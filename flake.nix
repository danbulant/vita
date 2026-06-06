{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    reenv.url = "github:levigross/NixRevAI";
    vitasdk.url = "github:sleirsgoevy/vitasdk.nix";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      reenv,
      vitasdk,
      fenix,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      ninfs = pkgs.python3Packages.buildPythonApplication {
        pname = "ninfs";
        version = "1.7b2";
        format = "setuptools";

        src = pkgs.fetchFromGitHub {
          owner = "ihaveamac";
          repo = "ninfs";
          rev = "v1.7b2";
          hash = "sha256-x1BxY3YGfCPdqp44xLnbKimRoMPNqel/bP1mGdvx+98=";
        };

        postPatch = ''
          sed -i '/#include <dlfcn.h>/a #include <string>' ninfs/hac/_crypto.cpp
        '';

        nativeBuildInputs = with pkgs.python3Packages; [
          setuptools
          wheel
        ];

        propagatedBuildInputs = with pkgs.python3Packages; [
          pycryptodomex
        ];

        doCheck = false;
      };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = with reenv.packages.${system}; [
          # pkgs.rust-analyzer-nightly
          fenix.packages.${system}.complete.toolchain
          fenix.packages.${system}.rust-analyzer

          vitasdk.packages.${system}.vitasdk
          vitasdk.packages.${system}.vitaGL
          bindiff
          ghidra-with-extensions
          retdec
          pkgs.bintools
          pkgs.binwalk
          pkgs.ctrtool
          pkgs.fuse
          pkgs.fuse3
          pkgs.mtools
          pkgs.openssl
          pkgs.p7zip
          pkgs.sleuthkit
          pkgs.aircrack-ng
          pkgs.hostapd
          pkgs.iw
          ninfs
          pkgs.tcpdump
          pkgs.wireshark-cli
          (pkgs.python3.withPackages (pythonPackages: [
            pythonPackages.construct
            pythonPackages.cryptography
            pythonPackages.fusepy
            pythonPackages.mcp
            pythonPackages.pip
            pythonPackages.pycryptodome
            pythonPackages.pycryptodomex
            pythonPackages.pyfatfs
            pythonPackages.setuptools
            pythonPackages.wheel
          ]))
        ];

        shellHook = ''
          export GHIDRA_MCP_BRIDGE="${
            reenv.packages.${system}.ghidra-with-extensions
          }/libexec/ghidra-mcp/bridge_mcp_ghidra.py"
          export LD_LIBRARY_PATH="${
            pkgs.lib.makeLibraryPath [
              pkgs.fuse
              pkgs.fuse3
            ]
          }:''${LD_LIBRARY_PATH:-}"
        '';
      };
    };
}
