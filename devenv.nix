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
  # Pin a Node version with `package = pkgs.nodejs_22;` if needed.
  languages.javascript = {
    enable = true;
    pnpm.enable = true;
  };

  # protoc — required by the larkoapi build script (prost-build).
  packages = [
    pkgs.protobuf
    pkgs.pkg-config
  ];

  env.PROTOC = "${pkgs.protobuf}/bin/protoc";
}
