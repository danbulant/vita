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
          pkgs.p7zip
          (pkgs.python3.withPackages (pythonPackages: [
            pythonPackages.mcp
          ]))
        ];

        shellHook = ''
          export GHIDRA_MCP_BRIDGE="${reenv.packages.${system}.ghidra-with-extensions}/libexec/ghidra-mcp/bridge_mcp_ghidra.py"
        '';
      };
    };
}
