# fefix flake package
#
# `grammarOverlays` can be used to override the normal fefix `languages.toml`
# grammars with those from nixpkgs if that behavior is desired using the
# following snippet as an argument to the `.override` of this package. There is
# one extra setting, and that is the `passthru.SHARED_LIB` for the tree-sitter
# grammar. If the tree-sitter parser is built and is not in the file `parser`,
# then this can be set to the required path in the root of the package. Being
# that it is a `passthru` variable, it will not require a rebuild of the
# overridden package should that path change.
#
# Note: This may cause incompatibility overriding all of the grammars like this.
#       Be sure that the nixpkgs grammar works with the version of fefix being
#       built here!
#
# ```nix
# grammarOverlays = [
#   (final: prev:
#   builtins.mapAttrs (k: v:
#       pkgs.tree-sitter-grammars."tree-sitter-${k}" or v
#   ) prev)
# ];
# ```
{
  lib,
  rustPlatform,
  callPackage,
  runCommand,
  installShellFiles,
  git,
  formats,
  gitRev ? null,
  grammarOverlays ? [ ],
  includeGrammarIf ? _: true,
}:
let
  fs = lib.fileset;
  readTOML = f: with builtins; fromTOML (readFile f);
  cargoTOML = readTOML ./helix-term/Cargo.toml;
  wsTOML = readTOML ./Cargo.toml;

  src = fs.difference (fs.gitTracked ./.) (
    fs.unions [
      ./.envrc
      ./rustfmt.toml
      ./screenshot.png
      ./book
      ./docs
      ./runtime
      ./flake.lock
      ./languages.toml
      (fs.fileFilter (file: lib.strings.hasInfix ".git" file.name) ./.)
      (fs.fileFilter (file: file.hasExt "svg") ./.)
      (fs.fileFilter (file: file.hasExt "md") ./.)
      (fs.fileFilter (file: file.hasExt "nix") ./.)
    ]
  );

  # Remove the grammars built in `grammars.nix` from the languages.toml during
  # the build. Anything else is kept in for the user to build with `ffx -g`
  languages_all = readTOML ./languages.toml;
  languages_min = (formats.toml { }).generate "fefix-languages.toml" (
    (builtins.removeAttrs languages_all [ "grammar" ])
    // {
      grammar = builtins.filter (g: !builtins.elem g.name grammars.names) languages_all.grammar;
    }
  );

  # Next we actually need to build the grammars and the runtime directory
  # that they reside in. It is built by calling the derivation in the
  # grammars.nix file, then taking the runtime directory in the git repo
  # and hooking symlinks up to it.
  grammars = callPackage ./grammars.nix { inherit grammarOverlays includeGrammarIf; };
  runtimeDir = runCommand "fefix-runtime" { } ''
    mkdir -vp $out
    ln -vs -t $out ${./runtime}/*
    rm -vf $out/grammars
    ln -vs ${grammars} $out/grammars
  '';

  # Creates an 'unwrapped' fefix without references to the grammars
  fefix-unwrapped = rustPlatform.buildRustPackage (self: {
    pname = "fefix-unwrapped";
    version = wsTOML.workspace.package.version;
    src = fs.toSource {
      root = ./.;
      fileset = src;
    };

    cargoLock = {
      lockFile = ./Cargo.lock;
      # This is not allowed in nixpkgs but is very convenient here: it allows us to
      # avoid specifying `outputHashes` here for any git dependencies we might take
      # on temporarily.
      allowBuiltinFetchGit = true;
    };

    postPatch = ''
      cp -v ${languages_min} languages.toml
    '';

    nativeBuildInputs = [
      installShellFiles
      git
    ];

    buildType = "release";

    # fefix attempts to reach out to the network and get the grammars. Nix doesn't allow this.
    FEFIX_DISABLE_AUTO_GRAMMAR_BUILD = "1";
    # So fefix knows what rev it is.
    FEFIX_NIX_BUILD_REV = gitRev;

    doCheck = false;
    strictDeps = true;

    # Get all the application stuff in the output directory.
    postInstall = ''
      mkdir -p $out/lib
      installShellCompletion ${./contrib/completion}/ffx.{bash,fish,zsh}
      mkdir -p $out/share/{applications,icons/hicolor/{256x256,scalable}/apps}
      cp ${./contrib/Helix.desktop} $out/share/applications/Helix.desktop
      cp ${./logo.svg} $out/share/icons/hicolor/scalable/apps/fefix.svg
      cp ${./contrib/helix.png} $out/share/icons/hicolor/256x256/apps/fefix.png
    '';

    meta = {
      inherit (wsTOML.workspace.package) homepage;
      inherit (cargoTOML.package) description;
      mainProgram = cargoTOML.package.default-run;
      license = lib.licenses.mpl20;
    };
  });
in
# fefix falls back to the `runtime` directory next to its executable, so put the
# runtime there instead of setting `FEFIX_RUNTIME`, which would override the
# user's own and leak into child processes.
runCommand "fefix-${fefix-unwrapped.version}"
  {
    inherit (fefix-unwrapped) version meta;

    passthru = {
      unwrapped = fefix-unwrapped;
      inherit runtimeDir grammars;
    };
  }
  ''
    cp -r ${fefix-unwrapped} $out
    chmod -R u+w $out
    mkdir -p $out/libexec/fefix
    ln -s ${runtimeDir} $out/libexec/fefix/runtime
    for f in $out/bin/*; do
      mv "$f" $out/libexec/fefix/
      ln -s "../libexec/fefix/$(basename "$f")" "$f"
    done
  ''
