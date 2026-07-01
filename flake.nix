{
  description = "fredshell — an opinionated Rust shell";

  inputs = {
    precommit.url = "github:FredSystems/pre-commit-checks";
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    # PLAN_05 §4.5 — pinned reference toolchain for the spec harness.
    #
    # `bash` and `coreutils` from this input define the v1 oracle for
    # `cargo xtask spec record` and the bash-compat corpus. Pinned to
    # an explicit rev so the reference does not drift when the
    # floating `nixpkgs` input advances.
    #
    # Bump policy: bump deliberately, capturing the new versions in
    # `tests/spec/REFERENCE.md` and re-recording any affected fixtures
    # in the same commit. `cargo xtask spec versions` reports drift
    # between this pin and the floating `nixpkgs` as advisory output.
    nixpkgs-reference.url = "github:nixos/nixpkgs/536c906eb9a9a2a38e7a454f4a4ff254b1e6f493";

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
      nixpkgs-reference,
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

          referencePkgs = import nixpkgs-reference { inherit system; };

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

          # PLAN_05 §4.5 — pinned reference toolchain.
          # Re-exposed as flake packages so the spec harness and CI
          # can resolve them via `nix build .#bashReference` /
          # `.#coreutilsReference` rather than hardcoding store paths.
          bashReference = referencePkgs.bash;
          coreutilsReference = referencePkgs.coreutils;
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
              referencePkgs = import nixpkgs-reference { inherit system; };

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

                # PLAN_05 §4.5 — expose the pinned reference toolchain
                # to the spec harness via env vars. The harness reads
                # FREDSHELL_REFERENCE_BASH / FREDSHELL_REFERENCE_COREUTILS
                # instead of consulting PATH so the pin is explicit and
                # cannot be shadowed by the host system bash (e.g.
                # macOS bash 3.2).
                FREDSHELL_REFERENCE_BASH = "${referencePkgs.bash}/bin/bash";
                FREDSHELL_REFERENCE_COREUTILS = "${referencePkgs.coreutils}/bin";
                FREDSHELL_REFERENCE_BASH_VERSION = referencePkgs.bash.version;
                FREDSHELL_REFERENCE_COREUTILS_VERSION = referencePkgs.coreutils.version;
                FREDSHELL_FLOATING_BASH_VERSION = pkgs.bash.version;
                FREDSHELL_FLOATING_COREUTILS_VERSION = pkgs.coreutils.version;

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
