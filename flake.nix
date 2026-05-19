{
  description = "fredshell — an opinionated Rust shell";

  inputs = {
    precommit.url = "github:FredSystems/pre-commit-checks";
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      precommit,
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      inherit (nixpkgs) lib;
      systems = precommit.lib.supportedSystems;
    in
    {
      ##########################################################################
      ## OVERLAY — adds `pkgs.fredshell` when applied
      ##########################################################################
      overlays.default = import ./nix/overlay.nix { fredshell-flake = self; };

      ##########################################################################
      ## HOME-MANAGER MODULE — `programs.fredshell` option set
      ##########################################################################
      homeManagerModules.default = import ./nix/home-manager-module.nix { fredshell-flake = self; };

      packages = lib.genAttrs systems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default;

          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          };
        in
        {
          fredshell = rustPlatform.buildRustPackage {
            pname = "fredshell";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.makeWrapper
            ];

            buildInputs = [ ];

            # The fredshell binary lives in the `fredshell` crate.
            cargoBuildFlags = [
              "-p"
              "fredshell"
            ];

            meta = with pkgs.lib; {
              description = "An opinionated, batteries-included Rust shell";
              homepage = "https://github.com/fredsystems/fredshell";
              license = licenses.mit;
              mainProgram = "fredshell";
              platforms = platforms.unix;
            };
          };
        }
      );

      ##########################################################################
      ## CHECKS — unified base+rust via mkCheck
      ##########################################################################
      checks = builtins.listToAttrs (
        map (system: {
          name = system;
          value = {
            pre-commit-check = precommit.lib.mkCheck {
              inherit system;
              src = ./.;
              check_rust = true;
              enableXtask = true;
              rust_options = {
                xtaskCheck = "pc";
              };
              extraExcludes = [
                "typos.toml"
              ];
            };
          };
        }) systems
      );

      ##########################################################################
      ## DEV SHELLS
      ##########################################################################
      devShells = builtins.listToAttrs (
        map (system: {
          name = system;

          value =
            let
              pkgs = import nixpkgs { inherit system; };

              chk = self.checks.${system}."pre-commit-check";

              corePkgs = chk.enabledPackages or [ ];

              extraRustTools = [
                chk.passthru.devPackages
                pkgs.cargo-deny
                pkgs.cargo-machete
                pkgs.cargo-make
                pkgs.cargo-flamegraph
                pkgs.typos
                pkgs.markdownlint-cli2
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
                pkgs.cargo-llvm-cov
                pkgs.perf
              ];

              extraDev = chk.passthru.devPackages or [ ];
            in
            {
              default = pkgs.mkShell {
                buildInputs = extraRustTools ++ corePkgs ++ extraDev;

                shellHook = ''
                  ${chk.shellHook}

                  alias pre-commit="pre-commit run --all-files"
                '';
              };
            };
        }) systems
      );
    };
}
