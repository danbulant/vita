{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    reenv.url = "github:levigross/NixRevAI";
  };

  outputs =
    { nixpkgs, reenv, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = with reenv.packages.${system}; [
          bindiff
          ghidra-with-extensions
          retdec
          pkgs.bintools
          pkgs.binwalk
          pkgs.ctrtool
          pkgs.fuse3
          pkgs.mtools
          pkgs.openssl
          pkgs.p7zip
          pkgs.sleuthkit
          pkgs.aircrack-ng
          pkgs.hostapd
          pkgs.iw
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
          export GHIDRA_MCP_BRIDGE="${reenv.packages.${system}.ghidra-with-extensions}/libexec/ghidra-mcp/bridge_mcp_ghidra.py"
        '';
      };
    };
}
