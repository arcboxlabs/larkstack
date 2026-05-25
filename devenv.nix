{ pkgs, ... }:

{
  # Rust toolchain — stable + extras for clippy/fmt/IDE + wasm target for cf-worker.
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
    targets = [ "wasm32-unknown-unknown" ];
  };

  # protoc — required by the larkoapi build script (prost-build).
  packages = [
    pkgs.protobuf
    pkgs.pkg-config
  ];

  env.PROTOC = "${pkgs.protobuf}/bin/protoc";
}
