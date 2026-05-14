cask "vmux" do
  version "0.0.6"
  sha256 "e131e60a3b4a5c8992dcbbdfa1e16717345dc1f42b0e251bfe223664464a67df"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.6/Vmux_0.0.6_aarch64.dmg"
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
