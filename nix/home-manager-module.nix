# Home-manager module for fredshell.
#
# Usage (in your home-manager config):
#
#   imports = [ fredshell.homeManagerModules.default ];
#
#   programs.fredshell = {
#     enable = true;
#     defaultShell = true;       # set as $SHELL / chsh target
#     settings = {
#       prompt.preset = "starship-like";
#       ai.enable = true;
#     };
#   };
{ fredshell-flake }:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  inherit (lib)
    mkEnableOption
    mkOption
    mkIf
    types
    ;
  cfg = config.programs.fredshell;

  defaultPackage = fredshell-flake.packages.${pkgs.stdenv.hostPlatform.system}.fredshell;

  tomlFormat = pkgs.formats.toml { };

  configAttrset = {
    version = 1;
    managed_by = "home-manager";
  }
  // cfg.settings;
in
{
  options.programs.fredshell = {
    enable = mkEnableOption "fredshell";

    package = mkOption {
      type = types.package;
      default = defaultPackage;
      defaultText = lib.literalExpression "fredshell.packages.\${pkgs.stdenv.hostPlatform.system}.fredshell";
      description = "The fredshell package to install.";
    };

    defaultShell = mkOption {
      type = types.bool;
      default = false;
      description = ''
        When true, set fredshell as the user's login shell via
        `home.sessionVariables.SHELL`. Note that adding to /etc/shells
        requires a NixOS module (system-level), not home-manager.
      '';
    };

    settings = mkOption {
      inherit (tomlFormat) type;
      default = { };
      description = "Arbitrary settings written to fredshell's config.toml.";
    };
  };

  config = mkIf cfg.enable {
    home = {
      packages = [ cfg.package ];

      file."Library/Application Support/fredshell/config.toml" = lib.mkIf pkgs.stdenv.isDarwin {
        source = tomlFormat.generate "fredshell-config" configAttrset;
      };

      sessionVariables = lib.mkIf cfg.defaultShell {
        SHELL = "${cfg.package}/bin/fredshell";
      };
    };

    xdg.configFile."fredshell/config.toml" = lib.mkIf (!pkgs.stdenv.isDarwin) {
      source = tomlFormat.generate "fredshell-config" configAttrset;
    };
  };
}
