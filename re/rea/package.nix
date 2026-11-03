{
  lib,
  stdenv,
  buildNpmPackage,
  fetchurl,
  nodejs_22,
  jq,
  makeWrapper,
  autoPatchelfHook,
  procps,
}:

buildNpmPackage rec {
  pname = "rea-agents";
  version = "6.8.0";

  # The published npm tarball ships a prebuilt `dist/`, so no TypeScript build is needed.
  src = fetchurl {
    url = "https://registry.npmjs.org/rea-agents/-/rea-agents-${version}.tgz";
    hash = "sha512-9QDeRnGqEDzABZyr04eLnBXBv/c6I9/TWu/6M0MD4pEfkySI/BIgwTODIbAReX3C1x6hjw9+Jlx4A2yUSPZ9ww==";
  };

  sourceRoot = "package";

  nodejs = nodejs_22;

  # The tarball has no lockfile. The vendored one was generated with
  # `npm install --package-lock-only` after stripping devDependencies,
  # overrides and the optional `frida` dependency (which downloads binaries at install time).
  postPatch = ''
    ${lib.getExe jq} 'del(.devDependencies, .overrides, .optionalDependencies.frida, .scripts.prepare, .scripts.prepack)' \
      package.json > package.json.tmp
    mv package.json.tmp package.json
    cp ${./package-lock.json} package-lock.json
  '';

  npmDepsHash = "sha256-1cJWEdMzR+teRVbbTvmnz7+pOjC8BD5duN8ZhrgDcDc=";

  dontNpmBuild = true;
  npmFlags = [ "--ignore-scripts" ];

  nativeBuildInputs = [ makeWrapper ] ++ lib.optionals stdenv.hostPlatform.isLinux [ autoPatchelfHook ];
  # For the prebuilt node-pty binary.
  buildInputs = [ stdenv.cc.cc.lib ];

  # REA needs a procps-compatible `ps` on PATH for process ownership checks on Linux.
  postFixup = ''
    for bin in $out/bin/*; do
      wrapProgram "$bin" --suffix PATH : ${lib.makeBinPath [ procps ]}
    done
  '';

  meta = {
    description = "Reverse engineer anything from your terminal or agent with one CLI and MCP server";
    homepage = "https://github.com/morluto/rea";
    license = lib.licenses.mit;
    mainProgram = "rea";
  };
}
