cask "vmux" do
  version "0.0.10"
  sha256 "5ff7630882f03559dd67c20b65fe47e7ac3733b6290bf64da2623de39c3761c2"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.10/Vmux_0.0.10_aarch64.dmg"
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
