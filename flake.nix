{
  description = "Reedline - A readline-like crate for CLI text input (with LSP diagnostics)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      nixpkgs,
      crane,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      craneLib = crane.mkLib pkgs;

      # Filter for .md files (used by include_str! for documentation)
      mdFilter = path: _type: builtins.match ".*\\.md$" path != null;

      src = pkgs.lib.cleanSourceWith {
        src = ./.;
        filter =
          path: type:
          (mdFilter path type) || (craneLib.filterCargoSources path type);
      };

      commonArgs = {
        inherit src;
        pname = "reedline";
        version = "0.44.0-lsp";

        nativeBuildInputs = with pkgs; [ pkg-config ];
        buildInputs = with pkgs; [ ];

        # Build with lsp_diagnostics feature
        cargoExtraArgs = "--features lsp_diagnostics";
      };

      # Build dependencies separately - this gets cached
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in
    {
      packages.${system} = {
        default = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            doCheck = false;

            meta = {
              description = "A readline-like crate for CLI text input (with LSP diagnostics)";
              homepage = "https://github.com/nushell/reedline";
            };
          }
        );

        # Export the cargoArtifacts for nushell to reuse
        cargoArtifacts = cargoArtifacts;
      };

      # Export source and build tools for nushell to include
      lib.${system} = {
        inherit src cargoArtifacts commonArgs craneLib;
      };
    };
}
