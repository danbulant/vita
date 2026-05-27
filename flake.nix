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
          ghidra-bin
          ghidra-mcp
          retdec
        ];
      };
    };
}
