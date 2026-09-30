cask "vmux" do
  version "0.0.35"
  sha256 "4762c616ef8eec474e8eb96e1cbf24a2c43babac9bbfe80b0c217092300c214a"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.35/Vmux_0.0.35_aarch64.dmg"
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
