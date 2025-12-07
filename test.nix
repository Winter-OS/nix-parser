system-version: {
  home.homeDirectory = "/home/quentin";
  winter = {
    update = {
      flake_config = "fw-laptop-16";
      flake_path = "/home/quentin/config";
    };
    auto-update.enable = true;
  };
  home.file.".config/BOE_CQ_______NE160QDM_NZ6.icm".source = ./home-manager/BOE_CQ_______NE160QDM_NZ6.icm;
  home.username = "quentin";
  imports = [
    ../../modules/home-manager
    ../../modules/home-manager/firefox
    ../../modules/home-manager/gnome
    ../../modules/home-manager/kdrive.nix
    ../../modules/home-manager/zed.nix
    ../../modules/home-manager/git.nix
    ../../modules/home-manager/dev.nix
    ../../modules/home-manager/shell.nix
    ../../modules/home-manager/office.nix
    ../../modules/home-manager/flake-script.nix
    ../../modules/home-manager/vscode.nix
    ../../modules/home-manager/vim.nix
    ./home-manager/zed-remote-folder.nix
  ];
  nixpkgs.config.allowUnfree = true;
  home.stateVersion = system-version;
  home.keyboard = {
    variant = "fr";
    layout = "fr";
  };
}