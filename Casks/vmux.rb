cask "vmux" do
  version "0.0.21"
  sha256 "794d905653a3ea08dab73ce779bb61c95160560fd2553d198e04cccf6241aed6"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.21/Vmux_0.0.21_aarch64.dmg"
  name "Vmux"
  desc "AI-native workspace combining browser and terminal panes"
  homepage "https://vmux.ai/"

  depends_on macos: :ventura

  app "Vmux.app"

  zap trash: [
    "~/Library/Application Support/ai.vmux.desktop",
    "~/Library/Caches/ai.vmux.desktop",
    "~/Library/Preferences/ai.vmux.desktop.plist",
  ]
end
