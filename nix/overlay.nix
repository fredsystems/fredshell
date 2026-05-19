# Overlay that adds the `fredshell` package to nixpkgs.
#
# Usage in a flake:
#   nixpkgs.overlays = [ fredshell.overlays.default ];
#
# Then `pkgs.fredshell` is available.
{ fredshell-flake }:
final: _prev: {
  inherit (fredshell-flake.packages.${final.stdenv.hostPlatform.system}) fredshell;
}
