{
  description = "Rust development shell using devshell, flake-parts, and fenix";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0";
    flake-parts.url = "github:hercules-ci/flake-parts";
    devshell = {
      url = "github:numtide/devshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      nixpkgs,
      flake-parts,
      devshell,
      fenix,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ ];

      flake = { };

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
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
          cargoManifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
          releaseVersion = "0.1.0";
          releaseArtifacts = {
            aarch64-darwin = {
              target = "aarch64-apple-darwin";
              hash = "sha256-NH8wA3mIPoZMqz6XWgrNGlgVug2USRJy3jUe/f/x6cU=";
            };
            x86_64-darwin = {
              target = "x86_64-apple-darwin";
              hash = "sha256-Tbupw3evx9BHF/ilGoIIQoy9haO0OW2zgNdaGfB92NM=";
            };
            x86_64-linux = {
              target = "x86_64-unknown-linux-gnu";
              hash = "sha256-ozjmJRaojFDZmDuUT7vmcKNw2/Opl91UsPvxSEMUUg4=";
            };
          };
          releaseArtifact = releaseArtifacts.${system} or null;
          rustToolchain = fenix.packages.${system}.stable.withComponents [
            "cargo"
            "clippy"
            "rust-analyzer"
            "rust-src"
            "rustc"
            "rustfmt"
          ];
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          };
          octaSource = rustPlatform.buildRustPackage {
            pname = "octa";
            version = cargoManifest.package.version;
            src = pkgs.lib.cleanSource ./.;
            cargoLock.lockFile = ./Cargo.lock;
            SQLX_OFFLINE = "true";
            nativeBuildInputs = [ pkgs.makeWrapper ];
            nativeCheckInputs = [ pkgs.git ];

            postInstall = ''
              wrapProgram "$out/bin/octa" \
                --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.git ]}
            '';

            meta = {
              description = "Local Issue collaboration for developers and AI agents";
              homepage = "https://github.com/choplin/octa";
              license = pkgs.lib.licenses.mit;
              mainProgram = "octa";
            };
          };
          octaBin =
            if releaseArtifact == null then
              null
            else
              pkgs.stdenvNoCC.mkDerivation {
                pname = "octa-bin";
                version = releaseVersion;
                src = pkgs.fetchurl {
                  url = "https://github.com/choplin/octa/releases/download/v${releaseVersion}/octa-cli-${releaseArtifact.target}.tar.xz";
                  inherit (releaseArtifact) hash;
                };
                sourceRoot = "octa-cli-${releaseArtifact.target}";
                nativeBuildInputs = [ pkgs.makeWrapper ]
                  ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.autoPatchelfHook ];
                buildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.stdenv.cc.cc.lib ];

                installPhase = ''
                  runHook preInstall

                  install -Dm755 octa "$out/bin/octa"
                  install -Dm644 README.md "$out/share/doc/octa/README.md"
                  install -Dm644 CHANGELOG.md "$out/share/doc/octa/CHANGELOG.md"
                  install -Dm644 LICENSE-MIT "$out/share/licenses/octa/LICENSE-MIT"
                  wrapProgram "$out/bin/octa" \
                    --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.git ]}

                  runHook postInstall
                '';

                meta = {
                  description = "Local Issue collaboration for developers and AI agents";
                  homepage = "https://github.com/choplin/octa";
                  license = pkgs.lib.licenses.mit;
                  mainProgram = "octa";
                  sourceProvenance = [ pkgs.lib.sourceTypes.binaryNativeCode ];
                };
              };
        in
        {
          _module.args.pkgs = import nixpkgs {
            inherit system;
            overlays = [ devshell.overlays.default ];
          };

          devShells.default = pkgs.devshell.mkShell {
            imports = [ (pkgs.devshell.importTOML ./devshell.toml) ];
            packages = [
              rustToolchain
              pkgs.sqlite
              pkgs.sqlx-cli
            ];
          };

          packages = {
            octa = octaSource;
          }
          // pkgs.lib.optionalAttrs (octaBin != null) {
            octa-bin = octaBin;
          };
        };
    };
}
