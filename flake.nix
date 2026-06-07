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

      vita-parse-core =
        let
          python = pkgs.python3.withPackages (pythonPackages: [
            pythonPackages.pyelftools
          ]);
        in
        pkgs.stdenvNoCC.mkDerivation {
          pname = "vita-parse-core";
          version = "2022-04-05";

          src = pkgs.fetchFromGitHub {
            owner = "xyzz";
            repo = "vita-parse-core";
            rev = "644b5f081c5f3c9b205180793ab8f4209dfd9d97";
            hash = "sha256-SQCH4k7dpDEF6qS8oiDR1+PFm+1V2ePVixF4H0NpDNc=";
          };

          nativeBuildInputs = [ pkgs.makeWrapper ];

          postPatch = ''
            cat > util.py <<'PY'
            import string
            import struct


            def u16(buf, off):
                return struct.unpack("<H", bytes(buf[off:off + 2]))[0]


            def u32(buf, off):
                return struct.unpack("<I", bytes(buf[off:off + 4]))[0]


            def c_str(buf, off):
                end = off
                while end < len(buf) and buf[end] != 0:
                    end += 1
                return bytes(buf[off:end]).decode("utf-8", errors="replace")


            def hexdump(src, length=16, sep='.'):
                display = string.digits + string.ascii_letters + string.punctuation
                filter_chars = "".join((x if x in display else '.') for x in map(chr, range(256)))
                lines = []
                for c in range(0, len(src), length):
                    chars = bytes(src[c:c + length])
                    hex_bytes = " ".join(["%02x" % x for x in chars])
                    if len(hex_bytes) > 24:
                        hex_bytes = "%s %s" % (hex_bytes[:24], hex_bytes[24:])
                    printable = "".join(["%s" % filter_chars[x] for x in chars])
                    lines.append("%08x:  %-*s  |%s|\n" % (c, length * 3, hex_bytes, printable))
                print("".join(lines))
            PY

            substituteInPlace elf.py \
              --replace-fail "from elftools.common.py3compat import str2bytes, bytes2str" \
                $'def str2bytes(value):\n    return value if isinstance(value, bytes) else value.encode("utf-8")\n\ndef bytes2str(value):\n    return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value' \
              --replace-fail "return out.strip()" "return bytes2str(out.strip())"
          '';

          installPhase = ''
            runHook preInstall

            mkdir -p $out/share/vita-parse-core $out/bin
            cp -r . $out/share/vita-parse-core
            makeWrapper ${python}/bin/python $out/bin/vita-parse-core \
              --add-flags "$out/share/vita-parse-core/main.py"

            runHook postInstall
          '';
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
          vitasdk.packages.${system}.vitaShaRK
          vitasdk.packages.${system}.libmathneon
          vitasdk.packages.${system}.SceShaccCgExt
          vitasdk.packages.${system}.taihen
          vitasdk.packages.${system}.sdl2
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
          vita-parse-core
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
