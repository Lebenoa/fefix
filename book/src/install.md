# Installing fefix

The typical way to install fefix is via [your operating system's package manager](./package-managers.md).

Note that:

- To get the latest nightly version of fefix, you need to
  [build from source](./building-from-source.md).

- To take full advantage of fefix, install the language servers for your
  preferred programming languages. See the
  [wiki](https://github.com/helix-editor/helix/wiki/Language-Server-Configurations)
  for instructions.

## Pre-built binaries

Download pre-built binaries from the [GitHub Releases page](https://github.com/helix-editor/helix/releases).
The tarball contents include an `ffx` binary and a `runtime` directory.
To set up fefix:

1. Add the `ffx` binary to your system's `$PATH` to allow it to be used from the command line.
2. Copy the `runtime` directory to a location that `ffx` searches for runtime files. A typical location on Linux/macOS is `~/.config/fefix/runtime`.

To see the runtime directories that `ffx` searches, run `ffx --health`. If necessary, you can override the default runtime location by setting the `FEFIX_RUNTIME` environment variable.
