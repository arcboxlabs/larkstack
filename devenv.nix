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

  # protoc — required by the larkoapi build script (prost-build).
  packages = [
    pkgs.protobuf
    pkgs.pkg-config
  ];

  env.PROTOC = "${pkgs.protobuf}/bin/protoc";
}
