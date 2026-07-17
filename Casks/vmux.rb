cask "vmux" do
  version "0.0.25"
  sha256 "38f43c3c085a595677a96431fe852f2c57266afd1c00b63dfdcc7a7cce4abf78"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.25/Vmux_0.0.25_aarch64.dmg"
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
