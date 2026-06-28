{
  description = "Development shell for abacus";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, crane, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        lib = pkgs.lib;

        rustToolchain = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
          targets = [
            "x86_64-unknown-linux-gnu"
            "aarch64-unknown-linux-gnu"
          ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        bootstrapSrc = pkgs.fetchFromGitHub {
          owner = "twbs";
          repo = "bootstrap";
          rev = "v5.3.3";
          hash = "sha256-LHVVm2NqlWS4HEhtSehPuLu/8yjEUex5EmMEfd2u7v8=";
        };

        bootstrapLoadPath = pkgs.runCommand "abacus-bootstrap-load-path" { } ''
          mkdir -p "$out"
          ln -s "${bootstrapSrc}" "$out/bootstrap"
        '';

        cargoLock = ./Cargo.lock;
        nixCargoLock =
          let
            lock = builtins.fromTOML (builtins.readFile cargoLock);
          in
          lock // {
            package = map
              (pkg:
                if pkg.name == "highs-sys" && pkg.version == "1.11.0" then
                  pkg // { dependencies = [ "bindgen" "pkg-config" ]; }
                else
                  pkg)
              (builtins.filter
                (pkg: !(pkg.name == "cmake" && pkg.version == "0.1.54"))
                lock.package);
          };
        outputHashes = {
          "git+https://github.com/loiclec/fuzzcheck-rs#816f44962aa46dfbbb6dcc35875c3826cf037340" = "sha256-uIwDY3tnmrB71ufzvHZ5dHTaZDR4foMpoxBI7ulR0iY=";
        };

        cargoSrc = craneLib.cleanCargoSource ./.;

        cargoVendorDir = craneLib.vendorCargoDeps {
          cargoLockParsed = nixCargoLock;
          inherit outputHashes;
          src = cargoSrc;
          overrideVendorCargoPackage = pkg: drv:
            if pkg.name == "highs" then
              drv.overrideAttrs (old: {
                postPatch = (old.postPatch or "") + ''
                  substituteInPlace Cargo.toml \
                    --replace-fail \
                      '[dependencies.highs-sys]
version = "1.11.0"' \
                      '[dependencies.highs-sys]
version = "1.11.0"
default-features = false
features = ["discover"]'
                '';
              })
            else if pkg.name == "highs-sys" then
              drv.overrideAttrs (old: {
                postPatch = (old.postPatch or "") + ''
                  substituteInPlace build.rs \
                    --replace-fail \
                      '        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
' \
                      ""
                '';
              })
            else
              drv;
        };

        appSrc = lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let
              pathString = toString path;
            in
            (craneLib.filterCargoSources path type)
            || lib.hasInfix "/assets/scss/" pathString
            || lib.hasInfix "/migrations/" pathString
            || lib.hasInfix "/src/tournamentsim/regressions/" pathString
            || lib.hasSuffix ".lalrpop" pathString;
        };

        commonRustArgs = {
          pname = "abacus";
          version = "0.1.0";
          src = cargoSrc;
          cargoLockParsed = nixCargoLock;
          inherit cargoVendorDir;

          nativeBuildInputs = [
            pkgs.dart-sass
            pkgs.pkg-config
          ];

          buildInputs = [
            pkgs.highs
            pkgs.sqlite
          ];

          doCheck = false;
        };

        cargoArtifacts = craneLib.buildDepsOnly ((builtins.removeAttrs commonRustArgs [ "src" ]) // {
          pname = "abacus-deps";
          dummySrc = craneLib.mkDummySrc {
            src = cargoSrc;
            cargoLockParsed = nixCargoLock;
          };
        });

        abacus = craneLib.buildPackage (commonRustArgs // {
          src = appSrc;
          inherit cargoArtifacts;
          env = {
            ABACUS_BOOTSTRAP_LOAD_PATH = "${bootstrapLoadPath}";
          };
        });

        devBp88teamDb = pkgs.runCommand "abacus-dev-bp88team-db" { } ''
          mkdir -p "$out"
          export ABACUS_TESTDATA_DIR="${./src/bin}"
          export DATABASE_URL="$TMPDIR/bp88team.sqlite"
          "${abacus}/bin/testdata" "$DATABASE_URL" --teams --judges --rooms --rounds
          cp "$DATABASE_URL" "$out/bp88team.sqlite"
        '';

        devServeBp88team = pkgs.writeShellApplication {
          name = "abacus-dev-serve-bp88team";
          runtimeInputs = [ pkgs.sqlite ];
          text = ''
            db_path="''${DATABASE_URL:-$PWD/sqlite.db}"

            if [[ "$db_path" == ":memory:" ]]; then
              echo "dev-serve-bp88team requires a file-backed SQLite DATABASE_URL" >&2
              exit 2
            fi

            if [[ ! -f "$db_path" ]] || ! sqlite3 "$db_path" "select count(*) from tournaments where slug = 'bp88team';" 2>/dev/null | grep -qx "1"; then
              mkdir -p "$(dirname "$db_path")"
              cp "${devBp88teamDb}/bp88team.sqlite" "$db_path"
              chmod u+w "$db_path"
            fi

            export DATABASE_URL="$db_path"
            export ABACUS_COOPER_HEWITT_FONT_DIR="${pkgs.cooper-hewitt}/share/fonts/opentype"
            export ABACUS_CHARTER_FONT_DIR="${pkgs.font-bitstream-type1}/share/fonts/X11/otf"
            exec "${abacus}/bin/abacus" "$@"
          '';
        };
      in
      {
        packages = {
          default = abacus;
          inherit abacus;
        };

        apps = {
          default = flake-utils.lib.mkApp {
            drv = abacus;
          };
          dev-serve-bp88team = flake-utils.lib.mkApp {
            drv = devServeBp88team;
          };
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            pkg-config
            sqlite
            diesel-cli
            dart-sass
            curl
            gnutar
            cooper-hewitt
            font-bitstream-type1
          ];

          env = {
            DATABASE_URL = "sqlite.db";
            ABACUS_COOPER_HEWITT_FONT_DIR = "${pkgs.cooper-hewitt}/share/fonts/opentype";
            ABACUS_CHARTER_FONT_DIR = "${pkgs.font-bitstream-type1}/share/fonts/X11/otf";
          };

          shellHook = ''
            export PATH="$PWD/scripts:$PATH"
            export ABACUS_HOST_TRIPLE="$(rustc -vV | awk '/host:/ { print $2 }')"
            echo "abacus dev shell"
            echo "  nix run .#dev-serve-bp88team"
            echo "  abacus serve bp88team"
            echo "  abacus test"
          '';
        };
      });
}
