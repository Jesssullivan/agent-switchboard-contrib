{
  description = "agent-switchboard development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    # Supplies the exact Rust release rust-toolchain.toml pins (nixpkgs
    # 26.05 ships 1.95, below the workspace rust-version).
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      # R-C239: lab consumes `packages.<system>.swb` instead of building swb
      # with its own (older) nixpkgs rustc.
      packageSystems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];
      rustChannel = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml)).toolchain.channel;
      workspaceVersion = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
    in
    {
      # Bazel (via bazelisk and .bazelversion) is the build authority. Cargo,
      # rustfmt and clippy here are diagnostic mirrors only.
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            bazelisk
            cargo
            clippy
            gh
            git
            gitleaks
            jq
            just
            python3
            rsync
            rustc
            rustfmt
            trufflehog
          ];
        };
        # The tag-triggered release workflow (.github/workflows/release.yml)
        # runs its gates here: signature verification, release-check and the
        # registry readback. Bazelisk comes from the runner custody fact.
        # mkShellNoCC, not mkShell (R-C282): the Bazel image build runs in
        # this shell, and mkShell's stdenv puts the Nix gcc-wrapper on PATH
        # and in CC. rules_cc would then compile libsqlite3-sys with a
        # /nix/store compiler, the binary would embed /nix/store paths and a
        # Nix dynamic linker, and the image would not reproduce the approved
        # digest (R-C268). Without a stdenv C compiler, Bazel uses the
        # runner's host C toolchain, the same one the approved build used.
        release = pkgs.mkShellNoCC {
          packages = with pkgs; [
            coreutils
            curl
            git
            gnupg
            jq
            python3
          ];
        };
      });

      # The swb binary built with the toolchain rust-toolchain.toml pins
      # (1.97.1, the same release MODULE.bazel registers for Bazel). Bazel
      # stays the build and test authority: `just check` runs the tests, so
      # this derivation only compiles the binary.
      packages = nixpkgs.lib.genAttrs packageSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          toolchain = pkgs.rust-bin.stable.${rustChannel}.minimal;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
          swb = rustPlatform.buildRustPackage {
            pname = "swb";
            version = workspaceVersion;
            src = nixpkgs.lib.fileset.toSource {
              root = ./.;
              fileset = nixpkgs.lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./crates
                ./schemas
              ];
            };
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [
              "-p"
              "swb"
            ];
            doCheck = false;
            meta = {
              description = "agent-switchboard broker, agent daemon and hook client";
              mainProgram = "swb";
            };
          };
        in
        {
          inherit swb;
          default = swb;
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style);
    };
}
