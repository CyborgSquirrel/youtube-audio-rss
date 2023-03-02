{
  description = "A very basic flake";
  
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-22.11";
    nixpkgs-unstable.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  
  outputs = { self, nixpkgs, nixpkgs-unstable, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (final: prev:
             let 
               unstable = import nixpkgs-unstable {
                 inherit system;
               };
             in {
                yt-dlp = unstable.yt-dlp;
              }
            )
          ];
        };
      in {
        devShell = pkgs.mkShell rec {
          name = "shuter-dev";
          buildInputs = [
            pkgs.cargo
            pkgs.rustc
            pkgs.openssl
            pkgs.openssl.dev
            pkgs.yt-dlp
            pkgs.aria2
          ];
          nativeBuildInputs = [
            pkgs.pkg-config
          ];
          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
        };
      }
    );
}
