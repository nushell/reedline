{
  description = "A readline-like crate for CLI text input";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      flake-parts,
      rust-overlay,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem =
        {
          config,
          self',
          inputs',
          pkgs,
          system,
          ...
        }:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import inputs.nixpkgs {
            inherit system overlays;
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
            ];
          };

          nativeBuildInputs = with pkgs; [
            rustToolchain
            pkg-config
          ];

          buildInputs =
            with pkgs;
            [
              sqlite
              cargo-nextest
            ]
            ++ lib.optionals stdenv.hostPlatform.isLinux [
              wayland
              libxkbcommon
            ]
            ++ lib.optionals stdenv.hostPlatform.isDarwin [
              darwin.apple_sdk.frameworks.AppKit
            ];
        in
        {
          devShells.default = pkgs.mkShell {
            inherit buildInputs nativeBuildInputs;

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          };

          packages = {
            default = config.packages.reedline;

            reedline = pkgs.rustPlatform.buildRustPackage {
              pname = "reedline";
              version = "0.42.0";

              src = ./.;

              cargoLock = {
                lockFile = ./Cargo.lock;
              };

              inherit nativeBuildInputs buildInputs;

              meta = with pkgs.lib; {
                description = "A readline-like crate for CLI text input";
                homepage = "https://github.com/nushell/reedline";
                license = licenses.mit;
                maintainers = [ ];
              };
            };
          };
        };
    };
}
