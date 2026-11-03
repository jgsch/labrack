"""Tests for scripts/mockup.py. Run with `mise run test` (inside the Nix shell: Chromium, ImageMagick)."""

import json
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

import mockup

HTML = mockup.HTML
SRC = mockup.SRC

FONTS = [(f, w) for f, ws in (("barlow-condensed", (500, 600, 700)), ("ibm-plex-mono", (400, 500, 600))) for w in ws]


def committed_html() -> str:
    return HTML.read_text(encoding="utf-8")


def png_size(path: Path) -> str:
    return subprocess.run(
        ["magick", "identify", "-format", "%wx%h", str(path)], capture_output=True, text=True, check=True
    ).stdout


class Tree(unittest.TestCase):
    """ARCH-9, ARCH-10: spec/mockup/ and spec/mockup.html are two forms of the same mockup."""

    def test_tree_matches_html(self):
        self.assertEqual(sorted(mockup.unpack_html(committed_html())), sorted(mockup.read_tree(SRC)))
        for name, data in mockup.unpack_html(committed_html()).items():
            self.assertEqual(data, mockup.read_tree(SRC)[name], name)

    def test_pack_reproduces_html(self):
        self.assertEqual(mockup.pack_files(mockup.read_tree(SRC)), committed_html())

    def test_edit_survives_pack_and_unpack(self):
        files = mockup.read_tree(SRC)
        board = next(n for n in files if n.startswith("boards/"))
        self.assertIn(b"Lab.Rack", files[board])
        files[board] = files[board].replace(b"Lab.Rack", b"Lab.Rack EDITED", 1)
        again = mockup.unpack_html(mockup.pack_files(files))
        self.assertEqual(again, files)
        self.assertNotEqual(again, mockup.read_tree(SRC))

    def test_write_then_read_removes_stale_files(self):
        files = mockup.read_tree(SRC)
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            mockup.write_tree(files, tmp)
            (tmp / "assets" / "old.bin").write_bytes(b"x")
            mockup.write_tree(files, tmp)
            self.assertEqual(mockup.read_tree(tmp), files)

    def test_refuses_to_overwrite_a_foreign_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "notes.txt").write_text("mine")
            with self.assertRaises(mockup.Error):
                mockup.write_tree(mockup.read_tree(SRC), Path(tmp))

    def test_boards_follow_the_mockup(self):
        boards = mockup.boards(mockup.read_tree(SRC))
        self.assertEqual(len(boards), 9)
        self.assertEqual(boards[0].slug, "01-a-rack-amplifiers")
        self.assertEqual((boards[0].width, boards[0].height), (1200, 900))
        self.assertEqual({(b.width, b.height) for b in boards[1:]}, {(1200, 1100), (1200, 900)})
        self.assertEqual(len({b.slug for b in boards}), 9)


class Fonts(unittest.TestCase):
    """UX-9, PERF-4: both font families ship with the mockup, with their licences."""

    def test_font_files(self):
        files = mockup.read_tree(SRC)
        for family, weight in FONTS:
            data = files[f"assets/{family}-{weight}-latin.woff2"]
            self.assertTrue(data.startswith(b"wOF2"), (family, weight))

    def test_latin_faces_do_not_need_the_network(self):
        files = mockup.read_tree(SRC)
        for name, data in files.items():
            if not name.startswith("boards/"):
                continue
            faces = re.findall(r"/\* latin \*/\s*@font-face\s*\{(.*?)\}", data.decode(), re.S)
            self.assertEqual(len(faces), 6, name)
            for face in faces:
                self.assertRegex(face, r'url\("assets/[a-z0-9-]+-latin\.woff2"\)', name)

    def test_licenses(self):
        text = mockup.read_tree(SRC)["licenses.txt"].decode()
        for needle in ("Barlow Condensed", "IBM Plex Mono", "SIL OPEN FONT LICENSE"):
            self.assertIn(needle, text)

    def test_localize_fonts(self):
        files = mockup.read_tree(SRC)
        meta = json.loads(files["bundle.json"])
        remote = "https://fonts.gstatic.com/s/ibmplexmono/v20/test-400.woff2"
        for page in meta["pages"]:
            local = "assets/ibm-plex-mono-400-latin.woff2"
            files[page["file"]] = files[page["file"]].replace(f'url("{local}")'.encode(), f"url({remote})".encode())
            page["resources"] = [r for r in page["resources"] if r["file"] != local]
        files["bundle.json"] = (json.dumps(meta, indent=2, ensure_ascii=False) + "\n").encode()
        del files["assets/ibm-plex-mono-400-latin.woff2"]

        asked = []

        def fetch(url):
            asked.append(url)
            return b"wOF2fake"

        done = mockup.localize_fonts(files, fetch)
        self.assertEqual(asked.count(remote), 1, "downloaded once for all boards")
        self.assertEqual(done["assets/ibm-plex-mono-400-latin.woff2"], b"wOF2fake")
        for page in json.loads(done["bundle.json"])["pages"]:
            self.assertIn("assets/ibm-plex-mono-400-latin.woff2", [r["file"] for r in page["resources"]])
            self.assertIn(b'url("assets/ibm-plex-mono-400-latin.woff2")', done[page["file"]])
        self.assertEqual(mockup.unpack_html(mockup.pack_files(done)), done)
        asked.clear()
        self.assertEqual(mockup.localize_fonts(done, fetch), done, "second run changes nothing")
        self.assertEqual(asked, [])


class Render(unittest.TestCase):
    """ARCH-7: one screenshot per board, compared with ImageMagick."""

    @classmethod
    def setUpClass(cls):
        cls.files = mockup.read_tree(SRC)
        cls.tmp = tempfile.TemporaryDirectory()
        cls.out = Path(cls.tmp.name)
        cls.pngs = mockup.render(cls.files, cls.out / "a")

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_one_png_per_board_with_the_board_size(self):
        boards = mockup.boards(self.files)
        self.assertEqual([p.name for p in self.pngs], [f"{b.slug}.png" for b in boards])
        for board, png in zip(boards, self.pngs):
            self.assertEqual(png_size(png), f"{board.width}x{board.height}", png.name)

    def test_boards_show_different_pages(self):
        a, b = self.pngs[1], self.pngs[2]  # MIXER and LEVELS tab, both 1200x1100
        self.assertGreater(mockup.compare(a, b, fuzz="1%").ratio, 0.01)

    def test_rendering_is_repeatable(self):
        again = mockup.render(self.files, self.out / "b", only=["01-a-rack-amplifiers"])
        self.assertEqual(mockup.compare(self.pngs[0], again[0], fuzz="0%").differing, 0)

    def test_fonts_are_the_bundled_ones(self):
        # With the fonts removed from the page the text changes, so the shot differs from the real one.
        files = dict(self.files)
        meta = json.loads(files["bundle.json"])
        page = meta["pages"][0]
        for r in page["resources"]:
            if r["file"].startswith("assets/ibm-plex-mono-"):
                files[page["file"]] = files[page["file"]].replace(r["file"].encode(), b"assets/missing.woff2")
        broken = mockup.render(files, self.out / "c", only=["01-a-rack-amplifiers"])
        self.assertGreater(mockup.compare(self.pngs[0], broken[0], fuzz="1%").differing, 1000)


class Compare(unittest.TestCase):
    def png(self, tmp: Path, name: str, *draw: str) -> Path:
        path = tmp / name
        subprocess.run(["magick", "-size", "40x20", "xc:black", *draw, str(path)], check=True)
        return path

    def test_counts_differing_pixels(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            a = self.png(tmp, "a.png")
            same = self.png(tmp, "same.png")
            b = self.png(tmp, "b.png", "-fill", "white", "-draw", "rectangle 0,0 9,9")
            self.assertEqual(mockup.compare(a, same).differing, 0)
            r = mockup.compare(a, b)
            self.assertEqual((r.differing, r.total), (100, 800))
            self.assertAlmostEqual(r.ratio, 0.125)
            self.assertTrue(mockup.compare(a, b).within(0.2))
            self.assertFalse(mockup.compare(a, b).within(0.1))

    def test_different_sizes_are_an_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            a = self.png(tmp, "a.png")
            other = tmp / "other.png"
            subprocess.run(["magick", "-size", "10x10", "xc:black", str(other)], check=True)
            with self.assertRaises(mockup.Error):
                mockup.compare(a, other)

    def test_fuzz_ignores_small_color_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            a = self.png(tmp, "a.png")
            near = tmp / "near.png"
            subprocess.run(["magick", "-size", "40x20", "xc:#010101", str(near)], check=True)
            self.assertEqual(mockup.compare(a, near, fuzz="0%").differing, 800)
            self.assertEqual(mockup.compare(a, near, fuzz="2%").differing, 0)


if __name__ == "__main__":
    unittest.main()
