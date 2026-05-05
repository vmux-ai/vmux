cask "vmux" do
  version "0.0.1"
  sha256 "40b9c34e9b18878b9848ccd29efc5884f79286c27c006c0101611b70421fddb0"

  url "https://github.com/v0.0.1/Vmux_0.0.1_aarch64.dmg"
  name "Vmux"
  desc "AI-native workspace combining browser and terminal panes"
  homepage "https://vmux.ai"

  depends_on macos: ">= :ventura"

  app "Vmux.app"

  zap trash: [
    "~/Library/Application Support/ai.vmux.desktop",
    "~/Library/Caches/ai.vmux.desktop",
    "~/Library/Preferences/ai.vmux.desktop.plist",
  ]
end
