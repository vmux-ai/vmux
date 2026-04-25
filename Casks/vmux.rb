cask "vmux" do
  version "0.1.0"
  sha256 "PLACEHOLDER"

  url "https://github.com/vmux-ai/vmux/releases/download/v#{version}/PLACEHOLDER.dmg"
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
