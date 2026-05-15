cask "vmux" do
  version "0.0.8"
  sha256 "0c805515856d89249cf0e84ac399cff834f1b9c2f5d4808e60635cf04b22b564"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.8/Vmux_0.0.8_aarch64.dmg"
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
