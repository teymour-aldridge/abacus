{
  description = "Development shell for abacus";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
          targets = [
            "x86_64-unknown-linux-gnu"
            "aarch64-unknown-linux-gnu"
          ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            pkg-config
            sqlite
            diesel-cli
            nodejs
            dart-sass
            curl
            gnutar
          ];

          env = {
            DATABASE_URL = "sqlite.db";
          };

          shellHook = ''
            export PATH="$PWD/scripts:$PATH"
            export ABACUS_HOST_TRIPLE="$(rustc -vV | awk '/host:/ { print $2 }')"
            echo "abacus dev shell"
            echo "  abacus serve bp88team"
            echo "  abacus test"
          '';
        };
      });
}
