{ lib
, rustPlatform
, pkg-config
, openssl
, stdenv
}:

rustPlatform.buildRustPackage rec {
  pname = "digicode";
  version = "0.71.2";

  src = lib.cleanSource ../.;

  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "agentgrep-0.1.6" = "sha256-yBLs2YZ6cUlTHYZGLtlAXpK7/9xX2kPi46B1YLbuPUU=";
      "mermaid-rs-renderer-0.3.1" = "sha256-uekh1vJ19dAPP7+4PiqSlJizApZLpDhBWBoyN+fgS9s=";
    };
  };

  nativeBuildInputs = [ pkg-config ];
  buildInputs = lib.optionals stdenv.isLinux [ openssl ];

  cargoBuildFlags = [ "--bin" "digicode" ];
  cargoInstallFlags = [ "--bin" "digicode" ];

  # The compatibility name is deliberately an alias to the same immutable
  # executable. The runtime remains jcode-compatible.
  postInstall = ''
    ln -s digicode "$out/bin/jcode"
  '';

  meta = {
    description = "Digicode coding agent (maintained creatifcoding fork)";
    homepage = "https://github.com/creatifcoding/digicode";
    license = lib.licenses.mit;
    mainProgram = "digicode";
    platforms = lib.platforms.unix;
  };
}
