{
  description = "youtube-audio-rss flake";
  
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
                inherit (unstable) yt-dlp rustc cargo rustPlatform;
              }
            )
          ];
        };
      in rec {
        packages.default = pkgs.rustPlatform.buildRustPackage rec {
          name = "youtube-audio-rss";
          version = "0.1.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          buildInputs = [
            pkgs.openssl
            pkgs.openssl.dev
            pkgs.sqlite
          ];
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.cargo
            pkgs.rustc
            pkgs.makeBinaryWrapper
          ];
          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
          postFixup = ''
            wrapProgram $out/bin/youtube-audio-rss \
              --set PATH ${pkgs.lib.makeBinPath [
                pkgs.yt-dlp
                pkgs.aria2
              ]}
          '';
        };
        nixosModules.default = { config, lib, pkgs, ... }: let
          cfg = config.services.youtube-audio-rss;
        in {
          options.services.youtube-audio-rss = {
            enable = lib.mkOption {
              type = lib.types.bool;
              default = false;
              description = ''
                If enabled, will run a server to convert youtube channels to podcasts.
              '';
            };
            config = lib.mkOption {
              type = lib.types.lines;
              default = "";
              example = ''
                ip = "0.0.0.0"
                port = 4100

                working_dir = "/path/to/working_dir"
                youtube_secret_path = "/path/to/youtube_secret.json"

                audio_cache_size = "100M"
                youtube_update_delay = "15m"
                
                url_scheme = "http"
                url_path_prefix = "/youtube_channel_podcast"
              '';
            };
          };
          config = {
            systemd.services.youtube-audio-rss = lib.mkIf cfg.enable {
              description = "Convert youtube channels to podcasts.";
              serviceConfig = let
                configFile = pkgs.writeText "config.toml" cfg.config;
              in {
                WantedBy = [ "multi-user.target" ];
                ExecStart = "${pkgs.lib.getExe packages.default} --config-path ${configFile} --log INFO";
                Type = "exec";
                StandardError = "journal";
                StandardOutput = "journal";
              };
            };
          };
        };
        devShell = pkgs.mkShell rec {
          name = "youtube-audio-rss";
          buildInputs = [
            pkgs.openssl
            pkgs.openssl.dev
            pkgs.sqlite
          ];
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.cargo
            pkgs.rustc
          ];
          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
        };
      }
    );
}
