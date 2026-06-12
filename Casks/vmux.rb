cask "vmux" do
  version "0.0.15"
  sha256 "8a3008023718b16d623937e8a8b174e6a138fbaad50a092725ff17306ea5afbf"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.15/Vmux_0.0.15_aarch64.dmg"
  name "Vmux"
  desc "AI-native workspace combining browser and terminal panes"
  homepage "https://vmux.ai"

  depends_on macos: :ventura

  app "Vmux.app"

  zap trash: [
    "~/Library/Application Support/ai.vmux.desktop",
    "~/Library/Caches/ai.vmux.desktop",
    "~/Library/Preferences/ai.vmux.desktop.plist",
  ]
end
