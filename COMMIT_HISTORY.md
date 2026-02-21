0.11.117 - Added creation lore and Saturnian creation myth to lore page
feb3864 Add play button, crossbeam event channel, and piano keyboard
1444a69 Revert to egui editor with delta-based window resize
25bd5e6 Remove SlotMode, add piano keyboard plan, standalone launcher, UI polish
0556ea6 Fix Linux build: manual bundle from zigbuild artifacts
284418f Target glibc 2.31 for Linux builds via cargo-zigbuild
deb4e4a v0.2.0: sampler voice rendering, comprehensive tests
db778c7 Replace state/sequence with song variable model; add README
cd2b1a5 Reactive .sw execution model + preset-as-note syntax
6da20d3 Unify Mode 1/Mode 2 into single .sw slot architecture
bed9d10 add icons
3fc08aa chore: bump version to 0.1.1
7328526 docs: add version bump and release trigger policy to copilot instructions
4f6787a docs: add GitHub Actions verification to copilot instructions
3626cb9 docs: add copilot instructions
8fc05be Fix macOS CI: xtask always outputs to target/bundled, rename between arch builds
b349c4f Fix macOS CI: CLAP is a bundle dir on macOS, handle like VST3
578253d Fix macOS CI: use correct per-target bundle paths for lipo
793d629 Fix CI: use git clone for songwalker-core (actions/checkout path restriction)
fe8159d Fix compilation, add xtask bundling, add GitHub Actions release CI
fc24c5a Initial implementation: VST3/CLAP multi-timbral instrument plugin
