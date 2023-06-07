{
  description = "A build for proa.";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rusttoolchain =
          pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in rec {
        # `nix build`
        packages = {
          proa = pkgs.rustPlatform.buildRustPackage {
            pname = "proa";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ rusttoolchain pkgs.openssl ]
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin
              [ pkgs.darwin.apple_sdk.frameworks.Security ];
          };
        };
        defaultPackage = packages.proa;

        # nix develop
        devShell = pkgs.mkShell {
          buildInputs = with pkgs;
            [ rusttoolchain pkg-config skaffold kind openssl ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin
            [ pkgs.darwin.apple_sdk.frameworks.Security ];
        };

      });
}
