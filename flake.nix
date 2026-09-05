{
  description = "A post-modern text editor.";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    ...
  }: let
    inherit (nixpkgs) lib;
    eachSystem = lib.genAttrs lib.systems.flakeExposed;
    pkgsFor = eachSystem (system:
      import nixpkgs {
        localSystem.system = system;
        overlays = [(import rust-overlay) self.overlays.fefix];
      });
    gitRev = self.rev or self.dirtyRev or null;
  in {
    packages = eachSystem (system: {
      inherit (pkgsFor.${system}) fefix;
      /*
      The default fefix build. Uses the latest stable Rust toolchain, and unstable
      nixpkgs.

      The build inputs can be overridden with the following:

      packages.${system}.default.override { rustPlatform = newPlatform; };

      Overriding a derivation attribute can be done as well:

      packages.${system}.default.overrideAttrs { buildType = "debug"; };
      */
      default = self.packages.${system}.fefix;
    });
    checks =
      lib.mapAttrs (system: pkgs: let
        # Get fefix's MSRV toolchain to build with by default.
        msrvToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        msrvPlatform = pkgs.makeRustPlatform {
          cargo = msrvToolchain;
          rustc = msrvToolchain;
        };
      in {
        fefix = self.packages.${system}.fefix.override {
          rustPlatform = msrvPlatform;
        };
      })
      pkgsFor;

    # Devshell behavior is preserved.
    devShells =
      lib.mapAttrs (system: pkgs: {
        default = let
          commonRustFlagsEnv = "-C link-arg=-fuse-ld=lld -C target-cpu=native --cfg tokio_unstable";
          platformRustFlagsEnv = lib.optionalString pkgs.stdenv.isLinux "-Clink-arg=-Wl,--no-rosegment";
        in
          pkgs.mkShell {
            inputsFrom = [
              (self.checks.${system}.fefix.override {
                includeGrammarIf = _: false;
              })
            ];
            nativeBuildInputs = with pkgs;
              [
                lld
                cargo-flamegraph
                rust-bin.nightly.latest.rust-analyzer
                mdbook
              ]
              ++ (lib.optional (stdenv.isx86_64 && stdenv.isLinux) cargo-tarpaulin)
              ++ (lib.optional stdenv.isLinux lldb);
            shellHook = ''
              export RUST_BACKTRACE="1"
              export RUSTFLAGS="''${RUSTFLAGS:-""} ${commonRustFlagsEnv} ${platformRustFlagsEnv}"
            '';
          };
      })
      pkgsFor;

    overlays = {
      helix = final: prev: {
        helix = final.callPackage ./default.nix {inherit gitRev;};
      };

      default = self.overlays.fefix;
    };
  };
  nixConfig = {
    extra-substituters = ["https://helix.cachix.org"];
    extra-trusted-public-keys = ["helix.cachix.org-1:ejp9KQpR1FBI2onstMQ34yogDm4OgU2ru6lIwPvuCVs="];
  };
}
