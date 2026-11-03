#!/usr/bin/env python3
"""Tools for the UI mockup, spec/mockup.html (see plan/changes/mockup-tooling.md).

The mockup is a bundle: a loader script, a manifest of gzipped resources and a template. Its nine
boards are bundles themselves (each with its own manifest of fonts and scripts). `unpack` turns it into
a tree of plain files in spec/mockup/ and `pack` builds the single HTML file again:

    bundle.json        which board uses which resource (ids, mime types, compression)
    licenses.txt       the licence text of the bundled fonts (the comment at the top of the bundle)
    shell/root.html    loader of the outer bundle, shell/page.html: loader of a board
    index.html         the page that lists the boards
    boards/NN-name.html  the template of a board (markup, CSS, the prototype's script)
    assets/            fonts and scripts, shared by the boards

`fonts` makes the Latin font faces local, `render` takes a screenshot of every board and `compare`
counts the pixels two screenshots differ in. Needs Python 3, Chromium and ImageMagick (Nix shell).
"""

import argparse
import base64
import concurrent.futures
import dataclasses
import gzip
import hashlib
import html
import json
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import uuid
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parent.parent
HTML = ROOT / "spec" / "mockup.html"
SRC = ROOT / "spec" / "mockup"
OUT = ROOT / "target" / "mockup"

FORMAT = 1
BLOCKS = ("manifest", "ext_resources", "page_order", "template")
BLOCK_RE = re.compile(
    r'(<script type="__bundler/(manifest|ext_resources|page_order|template)">\n)([^\n]*)(\n\s*</script>)'
)
MARKER_RE = re.compile(r"@@(\w+)@@")
UUID_RE = r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
TREE_PATHS = ("bundle.json", "licenses.txt", "index.html", "shell", "boards", "assets")

PLEX_LICENSE_URL = "https://raw.githubusercontent.com/IBM/plex/master/LICENSE.txt"


class Error(Exception):
    pass


# ---- bundle <-> text ---------------------------------------------------------------------------------


def split_bundle(text: str) -> tuple[str, dict[str, str]]:
    """The bundle without its four data blocks (their place is `@@name@@`), and the blocks."""
    data: dict[str, str] = {}

    def take(m: re.Match) -> str:
        data[m.group(2)] = m.group(3)
        return f"{m.group(1)}@@{m.group(2)}@@{m.group(4)}"

    shell = BLOCK_RE.sub(take, text)
    if sorted(data) != sorted(BLOCKS):
        raise Error(f"not a bundle: found the blocks {sorted(data)}")
    return shell, data


def fill(shell: str, values: dict[str, str]) -> str:
    """Replace every `@@name@@` of the shell in one pass (values are not searched again)."""
    names = set(MARKER_RE.findall(shell))
    if names != set(values):
        raise Error(f"shell has the markers {sorted(names)}, got values for {sorted(values)}")
    return MARKER_RE.sub(lambda m: values[m.group(1)], shell)


def json_compact(value) -> str:
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def json_string(text: str, ascii_only: bool) -> str:
    """A template as the bundle stores it: a JSON string that cannot close the script tag.

    The outer bundle keeps non-ASCII characters, the boards write them as \\uXXXX escapes.
    """
    return json.dumps(text, ensure_ascii=ascii_only).replace("</", "<\\u002F")


def gzip_b64(data: bytes) -> str:
    return base64.b64encode(gzip.compress(data, 9, mtime=0)).decode()


def slugify(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def dump_meta(meta: dict) -> bytes:
    return (json.dumps(meta, indent=2, ensure_ascii=False) + "\n").encode()


# ---- unpack ------------------------------------------------------------------------------------------


def asset_name(rid: str, mime: str, raw: bytes, template: str, ext: dict[str, str]) -> str:
    """A readable file name for a resource of a board."""
    if rid in ext:
        return ext[rid].rsplit("/", 1)[-1]
    face = re.search(
        r"/\* ([a-z0-9-]+) \*/\s*@font-face\s*\{[^}]*?font-family: '([^']+)'[^}]*?font-weight: (\d+)"
        r'[^}]*?url\("' + re.escape(rid) + r'"\)',
        template,
        re.S,
    )
    if face:
        return f"{slugify(face.group(2))}-{face.group(3)}-{face.group(1)}.{mime.rsplit('/', 1)[-1]}"
    if raw.startswith(b"// GENERATED from dc-runtime"):
        return "dc-runtime.js"
    ext_of = {"text/javascript": "js", "text/css": "css"}
    return f"{rid[:8]}.{ext_of.get(mime, mime.rsplit('/', 1)[-1])}"


def unpack_html(text: str) -> dict[str, bytes]:
    """The files of the tree for a bundle (relative path -> content)."""
    shell, data = split_bundle(text)
    manifest = json.loads(data["manifest"])
    order = json.loads(data["page_order"])
    template = json.loads(data["template"])
    if json.loads(data["ext_resources"]) != []:
        raise Error("the outer bundle has external resources; not supported")
    if list(manifest) != order:
        raise Error("the manifest of the outer bundle is not in page order")
    comment = re.search(r"<!--(.*?)-->", shell, re.S)
    if not comment:
        raise Error("no licence comment in the bundle")
    files: dict[str, bytes] = {
        "licenses.txt": comment.group(1).encode(),
        "shell/root.html": (
            shell[: comment.start()] + "<!--@@licenses@@-->" + shell[comment.end() :]
        ).encode(),
    }
    titles = {
        i: html.unescape(t) for t, i in re.findall(rf'<h2>(.*?)</h2><iframe src="about:blank#({UUID_RE})"', template)
    }

    pages, page_shell, by_hash = [], None, {}
    for n, pid in enumerate(order, 1):
        entry = manifest[pid]
        if list(entry) != ["mime", "compressed", "data"] or entry["mime"] != "text/html" or not entry["compressed"]:
            raise Error(f"unexpected manifest entry for board {pid}")
        ptext = gzip.decompress(base64.b64decode(entry["data"])).decode()
        pshell, pdata = split_bundle(ptext)
        ptemplate = json.loads(pdata["template"])
        if json.loads(pdata["page_order"]) != []:
            raise Error(f"board {pid} contains boards")
        title = re.search(r"<title>(.*?)</title>", ptemplate).group(1)
        pshell = pshell.replace(f"<title>{title}</title>", "<title>@@title@@</title>").replace(
            f"Loading {title}...", "Loading @@title@@..."
        )
        if page_shell is None:
            page_shell = pshell
        elif pshell != page_shell:
            raise Error(f"board {pid} has a different loader than the first one")

        ext = {e["uuid"]: e["id"] for e in json.loads(pdata["ext_resources"])}
        resources, replaced = [], ptemplate
        for rid, res in json.loads(pdata["manifest"]).items():
            if list(res) != ["mime", "compressed", "data"]:
                raise Error(f"unexpected resource entry {rid}")
            raw = base64.b64decode(res["data"])
            raw = gzip.decompress(raw) if res["compressed"] else raw
            digest = hashlib.sha256(raw).hexdigest()
            if digest not in by_hash:
                name = "assets/" + asset_name(rid, res["mime"], raw, ptemplate, ext)
                if name in files:
                    stem, dot, suffix = name.rpartition(".")
                    name = f"{stem}-{digest[:8]}{dot}{suffix}"
                by_hash[digest] = name
                files[name] = raw
            path = by_hash[digest]
            if path in ptemplate:
                raise Error(f"{path} already occurs in the template of board {pid}")
            replaced = replaced.replace(rid, path)
            resources.append({"id": rid, "file": path, "mime": res["mime"], "compressed": res["compressed"]})
        by_id = {r["id"]: r["file"] for r in resources}
        slug = slugify(titles.get(pid, title))
        file = f"boards/{n:02d}-{slug}.html"
        if file in template:
            raise Error(f"{file} already occurs in the index template")
        template = template.replace(pid, file)
        files[file] = replaced.encode()
        pages.append(
            {
                "id": pid,
                "file": file,
                "resources": resources,
                "ext": [{"id": url, "file": by_id[rid]} for rid, url in ext.items()],
            }
        )
    files["shell/page.html"] = page_shell.encode()
    files["index.html"] = template.encode()
    files["bundle.json"] = dump_meta({"format": FORMAT, "pages": pages})
    return files


# ---- pack --------------------------------------------------------------------------------------------


def pack_page(files: dict[str, bytes], page: dict) -> str:
    template = files[page["file"]].decode()
    for r in sorted(page["resources"], key=lambda r: -len(r["file"])):
        template = template.replace(r["file"], r["id"])
    manifest, ids = {}, {}
    for r in page["resources"]:
        raw = files[r["file"]]
        manifest[r["id"]] = {
            "mime": r["mime"],
            "compressed": r["compressed"],
            "data": gzip_b64(raw) if r["compressed"] else base64.b64encode(raw).decode(),
        }
        ids[r["file"]] = r["id"]
    title = re.search(r"<title>(.*?)</title>", template).group(1)
    return fill(
        files["shell/page.html"].decode(),
        {
            "title": title,
            "manifest": json_compact(manifest),
            "ext_resources": json_compact([{"id": e["id"], "uuid": ids[e["file"]]} for e in page["ext"]]),
            "page_order": "[]",
            "template": json_string(template, ascii_only=True),
        },
    )


def pack_files(files: dict[str, bytes]) -> str:
    """The bundle (spec/mockup.html) for a tree."""
    meta = json.loads(files["bundle.json"])
    if meta.get("format") != FORMAT:
        raise Error(f"bundle.json has format {meta.get('format')}, expected {FORMAT}")
    manifest, template = {}, files["index.html"].decode()
    for page in meta["pages"]:
        manifest[page["id"]] = {
            "mime": "text/html",
            "compressed": True,
            "data": gzip_b64(pack_page(files, page).encode()),
        }
        template = template.replace(page["file"], page["id"])
    return fill(
        files["shell/root.html"].decode(),
        {
            "licenses": files["licenses.txt"].decode(),
            "manifest": json_compact(manifest),
            "ext_resources": "[]",
            "page_order": json_compact([p["id"] for p in meta["pages"]]),
            "template": json_string(template, ascii_only=False),
        },
    )


# ---- tree on disk ------------------------------------------------------------------------------------


def read_tree(src: Path) -> dict[str, bytes]:
    if not (src / "bundle.json").is_file():
        raise Error(f"{src} is not an unpacked mockup (no bundle.json)")
    return {p.relative_to(src).as_posix(): p.read_bytes() for p in sorted(src.rglob("*")) if p.is_file()}


def write_tree(files: dict[str, bytes], src: Path) -> None:
    """Write the tree, replacing the previous one. Refuses a directory that is not an unpacked mockup."""
    if src.exists() and any(src.iterdir()) and not (src / "bundle.json").is_file():
        raise Error(f"{src} exists and is not an unpacked mockup; refusing to overwrite it")
    for name in TREE_PATHS:
        path = src / name
        if path.is_dir():
            shutil.rmtree(path)
        elif path.exists():
            path.unlink()
    for name, data in files.items():
        path = src / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)


# ---- fonts -------------------------------------------------------------------------------------------

FACE_RE = re.compile(r"/\* latin \*/\s*@font-face\s*\{(.*?)\}", re.S)


def download(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "labrack-mockup/1"})
    with urllib.request.urlopen(request, timeout=60) as r:
        return r.read()


def localize_fonts(files: dict[str, bytes], fetch: Callable[[str], bytes] = download) -> dict[str, bytes]:
    """Make the Latin font faces of the boards local files.

    The mockup embeds Barlow Condensed but links IBM Plex Mono on fonts.gstatic.com. The Latin faces are
    downloaded (once) into assets/ and the boards point at them, so the mockup and the app need no
    network. The licence of a family that is added is appended to licenses.txt. Running it again changes
    nothing.
    """
    files = dict(files)
    meta = json.loads(files["bundle.json"])
    cache: dict[str, bytes] = {}
    for page in meta["pages"]:
        template = files[page["file"]].decode()
        for face in FACE_RE.findall(template):
            remote = re.search(r"url\((https://fonts\.gstatic\.com/[^)\s]+\.woff2)\)", face)
            if not remote:
                continue
            family = re.search(r"font-family: '([^']+)'", face).group(1)
            weight = re.search(r"font-weight: (\d+)", face).group(1)
            url, path = remote.group(1), f"assets/{slugify(family)}-{weight}-latin.woff2"
            if url not in cache:
                data = fetch(url)
                if not data.startswith(b"wOF2"):
                    raise Error(f"{url} is not a WOFF2 file")
                cache[url] = data
            if files.setdefault(path, cache[url]) != cache[url]:
                raise Error(f"{path} exists with other content")
            template = template.replace(f"url({url})", f'url("{path}")')
            if path not in [r["file"] for r in page["resources"]]:
                rid = str(uuid.uuid5(uuid.NAMESPACE_URL, f"{page['id']}/{path}"))
                page["resources"].append({"id": rid, "file": path, "mime": "font/woff2", "compressed": False})
            licenses = files["licenses.txt"].decode()
            if family not in licenses:
                if family != "IBM Plex Mono":
                    raise Error(f"no licence text known for {family}")
                text = fetch(PLEX_LICENSE_URL).decode().replace("\r\n", "\n").strip("\n")
                files["licenses.txt"] = (licenses.rstrip("\n") + f"\n\n\n{family}\n\n{text}\n\n").encode()
        files[page["file"]] = template.encode()
    files["bundle.json"] = dump_meta(meta)
    return files


# ---- render and compare ------------------------------------------------------------------------------


@dataclasses.dataclass
class Board:
    slug: str
    title: str
    width: int
    height: int
    html: str


# The prototype advances a clock every 100 ms to animate the meters unless its `live` property is false.
LIVE_RE = re.compile(r"(&quot;live&quot;:\{&quot;editor&quot;:&quot;boolean&quot;,&quot;default&quot;:)true")


def boards(files: dict[str, bytes], live: bool = False) -> list[Board]:
    """The boards in the order of the mockup, each as a page of its own, with the size of its frame.

    The pages are still (`live` false: no animation, so a screenshot is the same every time).
    """
    meta = json.loads(files["bundle.json"])
    if not live:
        files = dict(files)
        for page in meta["pages"]:
            files[page["file"]] = LIVE_RE.sub(r"\1false", files[page["file"]].decode()).encode()
    index = files["index.html"].decode()
    out = []
    for page in meta["pages"]:
        m = re.search(
            rf'<h2>(.*?)</h2><iframe src="about:blank#{re.escape(page["file"])}"[^>]*? width="(\d+)" height="(\d+)"',
            index,
        )
        if not m:
            raise Error(f"{page['file']} has no frame in index.html")
        out.append(
            Board(
                Path(page["file"]).stem,
                html.unescape(m.group(1)),
                int(m.group(2)),
                int(m.group(3)),
                pack_page(files, page),
            )
        )
    return out


def chromium() -> str:
    path = shutil.which("chromium") or shutil.which("chromium-browser") or shutil.which("google-chrome")
    if not path:
        raise Error("Chromium not found (run inside `nix develop`)")
    return path


def shoot(browser: str, page: Path, png: Path, width: int, height: int) -> None:
    with tempfile.TemporaryDirectory() as profile:
        cmd = [
            browser,
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=1",
            "--font-render-hinting=none",
            "--disable-lcd-text",
            f"--user-data-dir={profile}",
            f"--window-size={width},{height}",
            "--virtual-time-budget=8000",
            f"--screenshot={png}",
            page.as_uri(),
        ]
        run = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
    if not png.is_file():
        raise Error(f"no screenshot of {page.name}: {run.stderr.strip()[-400:]}")


def render(files: dict[str, bytes], out: Path, only: list[str] | None = None, jobs: int = 4) -> list[Path]:
    """Screenshot the boards (all, or those with these slugs) into out/<slug>.png; the pages go to out/pages/."""
    selected = [b for b in boards(files) if only is None or b.slug in only]
    if only is not None and len(selected) != len(set(only)):
        raise Error(f"unknown board in {only}")
    (out / "pages").mkdir(parents=True, exist_ok=True)
    browser = chromium()

    def one(board: Board) -> Path:
        page = out / "pages" / f"{board.slug}.html"
        page.write_text(board.html, encoding="utf-8")
        png = out / f"{board.slug}.png"
        png.unlink(missing_ok=True)
        shoot(browser, page, png, board.width, board.height)
        return png

    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        return list(pool.map(one, selected))


@dataclasses.dataclass
class Diff:
    differing: int
    total: int

    @property
    def ratio(self) -> float:
        return self.differing / self.total

    def within(self, max_ratio: float) -> bool:
        return self.ratio <= max_ratio


def magick(*args: str) -> str:
    run = subprocess.run(["magick", *args], capture_output=True, text=True)
    if run.returncode != 0:
        raise Error(f"magick {' '.join(args[:3])}: {run.stderr.strip()}")
    return run.stdout


def compare(a: Path, b: Path, fuzz: str = "1%", diff_png: Path | None = None) -> Diff:
    """How many pixels of two same-sized images differ in some channel by more than `fuzz`.

    This is not `compare -metric AE`: ImageMagick 7.1 reports a sum of colour differences there, not a
    pixel count, when the differences are small.
    """
    sizes = [magick("identify", "-format", "%w %h", str(p)).split() for p in (a, b)]
    if sizes[0] != sizes[1]:
        raise Error(f"cannot compare {a.name} ({'x'.join(sizes[0])}) and {b.name} ({'x'.join(sizes[1])})")
    cmd = [str(a), str(b), "-alpha", "off", "-compose", "difference", "-composite"]
    cmd += ["-separate", "-evaluate-sequence", "max", "-threshold", fuzz]
    if diff_png:
        cmd += ["-write", str(diff_png)]
    differing = magick(*cmd, "-format", "%[fx:round(mean*w*h)]", "info:")
    return Diff(int(float(differing)), int(sizes[0][0]) * int(sizes[0][1]))


# ---- command line ------------------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name, help_ in (
        ("unpack", "write the tree spec/mockup/ from spec/mockup.html (replaces the tree)"),
        ("pack", "build spec/mockup.html from the tree spec/mockup/"),
        ("fonts", "download the Latin faces the mockup links and use them from assets/"),
    ):
        p = sub.add_parser(name, help=help_)
        p.add_argument("--html", type=Path, default=HTML)
        p.add_argument("--src", type=Path, default=SRC)
    p = sub.add_parser("render", help="screenshot every board of the tree into target/mockup/")
    p.add_argument("--src", type=Path, default=SRC)
    p.add_argument("--out", type=Path, default=OUT)
    p.add_argument("--board", action="append", help="only this board (slug), may be repeated")
    p.add_argument("--jobs", type=int, default=4)
    p = sub.add_parser("compare", help="count the pixels in which two screenshots differ")
    p.add_argument("a", type=Path)
    p.add_argument("b", type=Path)
    p.add_argument("--fuzz", default="1%", help="colour distance that still counts as equal")
    p.add_argument("--max-ratio", type=float, default=0.0, help="allowed share of differing pixels")
    p.add_argument("--diff", type=Path, help="write an image of the differences")
    args = ap.parse_args(argv)

    try:
        if args.cmd == "unpack":
            files = unpack_html(args.html.read_text(encoding="utf-8"))
            write_tree(files, args.src)
            print(f"unpacked {len(files)} files from {args.html} to {args.src}")
        elif args.cmd == "pack":
            text = pack_files(read_tree(args.src))
            args.html.write_text(text, encoding="utf-8")
            print(f"packed {args.src} into {args.html} ({len(text)} bytes)")
        elif args.cmd == "fonts":
            files = read_tree(args.src)
            new = localize_fonts(files)
            write_tree(new, args.src)
            print("fonts already local" if new == files else f"added {len(set(new) - set(files))} files to {args.src}")
        elif args.cmd == "render":
            for png in render(read_tree(args.src), args.out, args.board, args.jobs):
                print(png)
        elif args.cmd == "compare":
            d = compare(args.a, args.b, args.fuzz, args.diff)
            print(f"{d.differing} of {d.total} pixels differ ({d.ratio:.4%})")
            return 0 if d.within(args.max_ratio) else 1
    except (Error, OSError, subprocess.SubprocessError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
