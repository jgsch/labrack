{
  description = "Lab.gruppen amplifier control (Rust)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      devShells = forAllSystems (
        pkgs:
        let
          # The only place the Rust version is set.
          rust = pkgs.rust-bin.stable."1.99.1".default.override {
            extensions = [
              "clippy"
              "rustfmt"
              "rust-analyzer"
              "rust-src"
            ];
          };
          # Libraries the Dioxus desktop window (WebKitGTK) links against.
          guiLibs = with pkgs; [
            webkitgtk_4_1
            gtk3
            libsoup_3
            glib
            openssl
            xdotool
          ];
        in
        {
          default = pkgs.mkShell {
            packages =
              [ rust ]
              ++ (with pkgs; [
                mise
                pkg-config
                dioxus-cli
                chromium
                imagemagick
                python3
              ])
              ++ guiLibs;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath guiLibs;
          };
        }
      );
    };
}
