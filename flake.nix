{
  description = "A very basic flake";
  
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-22.11";
    flake-utils.url = "github:numtide/flake-utils";
  };
  
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
      in {
        devShell = pkgs.mkShell rec {
          name = "shuter-dev";
          buildInputs = [
            pkgs.cargo
            pkgs.rustc
            pkgs.openssl
            pkgs.openssl.dev
          ];
          nativeBuildInputs = [
            pkgs.pkg-config
          ];
          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
        };
      }
    );
}
