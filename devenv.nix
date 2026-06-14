{ pkgs, ... }:

{
  # Rust toolchain — stable + extras for clippy/fmt/IDE.
  languages.rust = {
    enable = true;
    channel = "stable";
    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
      "rust-src"
    ];
  };

  # Frontend toolchain — Node.js + pnpm for the dashboard (dashboard/).
  languages.javascript = {
    enable = true;
    package = pkgs.nodejs_24;
    pnpm.enable = true;
  };

  # protoc — required by the larkoapi build script (prost-build).
  packages = [
    pkgs.protobuf
    pkgs.pkg-config
    pkgs.curl
    pkgs.prek
  ];

  env.PROTOC = "${pkgs.protobuf}/bin/protoc";

  # Refresh the pinned Linear GraphQL schema from Linear's published SDL (the
  # schema @linear/sdk is generated from). The committed file is a lock: builds
  # read it offline; run this after bumping queries or on a cadence, then commit.
  scripts.update-linear-schema = {
    description = "Refresh apps/integrations/linear/graphql/schema.graphql from Linear's SDK";
    exec = ''
      set -euo pipefail
      root="$(git rev-parse --show-toplevel)"
      dest="$root/apps/integrations/linear/graphql/schema.graphql"
      url="https://raw.githubusercontent.com/linear/linear/master/packages/sdk/src/schema.graphql"
      tmp="$(mktemp)"
      {
        echo "# Linear GraphQL schema — generated, DO NOT EDIT BY HAND."
        echo "# Source: https://github.com/linear/linear/blob/master/packages/sdk/src/schema.graphql"
        echo "# Refresh: run the update-linear-schema devenv script, then commit."
        echo
        curl -fsSL "$url"
      } > "$tmp"
      mv "$tmp" "$dest"
      echo "updated $dest ($(wc -l < "$dest") lines)"
    '';
  };

  scripts.console = {
    description = "Run the larkstack console backend on :8080";
    exec = ''
      cargo run -p console
    '';
  };

  scripts.dashboard = {
    description = "Run the dashboard Vite dev server";
    exec = ''
      root="$(git rev-parse --show-toplevel)"
      cd "$root/dashboard"
      pnpm dev
    '';
  };
}
