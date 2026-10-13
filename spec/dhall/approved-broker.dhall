-- Source of docs/releases/approved-broker.json. `just spec-dhall` (and the
-- flake check spec-dhall) renders this with dhall-to-json and fails unless it
-- is identical, after `jq -S`, to the committed JSON. Edit this file and the
-- JSON together; release-check.py keeps reading the JSON. SWB-R59 (R-C435, R-C655), R-C229.
let ApprovedRelease = ./ApprovedRelease.dhall

in    { file_sha256 =
          [ { mapKey = ".bazelversion", mapValue = "1b9487d55bea47fea50d226cc9c53bc548877ad2a318bac8fbc8b320f429e5c5" }
          , { mapKey = "BUILD.bazel", mapValue = "02051c3539cc0cba44203172c035d68e59a3e77ff279e4c227dfb8e9b8114780" }
          , { mapKey = "Cargo.lock", mapValue = "e839691209949dda5670ded2151c3a3c426fab288a0b2b67e0e265273fb3ccf6" }
          , { mapKey = "Cargo.toml", mapValue = "812ac065267e93259374311f24cdbb0ac444b53014d281fdc3877c0c253697e7" }
          , { mapKey = "MODULE.bazel", mapValue = "7639d2d00fd3bdab6e4dfb53d67e5d8d527a44d37ca9443c5c04a3ef2b0afe93" }
          , { mapKey = "MODULE.bazel.lock", mapValue = "d821fbd862082793591a20291dcef5017927d69875eb2fcb99228d08097fefd7" }
          , { mapKey = "cargo-bazel-lock.json", mapValue = "4ba0d70a1044b264f1b9e60c4584c0b3d734f8313639ab9c65a99f8306d2e033" }
          , { mapKey = "crates/swb-agentd/BUILD.bazel", mapValue = "4c53792c73662c8cf3737e36559acc042f53a28c4c5d638586e177df1f56adb7" }
          , { mapKey = "crates/swb-agentd/Cargo.toml", mapValue = "bd1e24061d70fa7e774eadcb87a83e81ff002e38c626603ba5d9e0462cfb0220" }
          , { mapKey = "crates/swb-agentd/src/lib.rs", mapValue = "03c8ff21db3b6556a4de5436ecf22589bf4ed43303cd6504b743394ad218bef7" }
          , { mapKey = "crates/swb-broker/BUILD.bazel", mapValue = "7bfc5fe7429424f4b9a9fd1c4a090854e78cdbf41a633d97bd61dddb3425accc" }
          , { mapKey = "crates/swb-broker/Cargo.toml", mapValue = "630fefce1b21b5b104f42c7a40bd36c6b965b8563be40ef37c75be69fca3531d" }
          , { mapKey = "crates/swb-broker/src/lib.rs", mapValue = "30ef533fd6d6eebaf22b18d886815ff5c5935bdaee14bff4dfbb925dcceee954" }
          , { mapKey = "crates/swb-broker/tests/stamp.rs", mapValue = "f07fa0a5d40fbb81aa4062354d87f3baf216aa03f66b15520ce0bb6897b117f3" }
          , { mapKey = "crates/swb-proto/BUILD.bazel", mapValue = "f886fc6c14a076e6f5996fb862f857e487454afcfe6271037988e52e03b35f1c" }
          , { mapKey = "crates/swb-proto/Cargo.toml", mapValue = "af016b200f0a54f49e374ad058825af9b79490b001af20e6136ab27d4f3fd4c3" }
          , { mapKey = "crates/swb-proto/src/lib.rs", mapValue = "6965fd48241e8a14969114a7154fcc984f562540f0229a36693b01d51a1cd1ec" }
          , { mapKey = "crates/swb-store/BUILD.bazel", mapValue = "f2c380aae0ff2697183171ceac6f3de5ddef63e4702899779e7a3829a7efa39d" }
          , { mapKey = "crates/swb-store/Cargo.toml", mapValue = "41e95b19c399e874202c2725a57425883381d96ac6e98072307702841fc0f437" }
          , { mapKey = "crates/swb-store/src/lib.rs", mapValue = "7614f3c91aa4201e16850817bda03963cf86ea067cf0302cbe4d581e961679fc" }
          , { mapKey = "crates/swb/BUILD.bazel", mapValue = "6eba47ac963e621f40a16d1327996021574cda3bddb592d1f38150c6af408215" }
          , { mapKey = "crates/swb/Cargo.toml", mapValue = "601bbc80f9ec9f76ee0be84a6ea6e8b264a291a1563b37910cd0f7c5fdf46cb1" }
          , { mapKey = "crates/swb/src/channel.rs", mapValue = "311e3e792cea8a8e28074f3ca765355ea5c94311d140e5f17c69e9bbe02dad16" }
          , { mapKey = "crates/swb/src/main.rs", mapValue = "beb7ac59967c3f92d66ff96fea1e1458f19b391eb2ed0ccb913387df223e2c9b" }
          , { mapKey = "deploy/BUILD.bazel", mapValue = "414e9a927e0dfb89f8236367db755014ce8b56b815621ee1d0f9da99a1265ac0" }
          , { mapKey = "platforms/BUILD.bazel", mapValue = "6d5c7d3dada37113084e621e8065a05e4e58e0c7dd7d206dfd7f0cb3f4507d25" }
          , { mapKey = "tools/BUILD.bazel", mapValue = "d7ae7983138ad3c50004efd36fb29cac5583647b5dc3f46189f594f2470bd31c" }
          , { mapKey = "tools/cc/BUILD.bazel", mapValue = "b4fdf94422a4c57858f0d456b6ae4460755ef2ab061768861086440f18b45436" }
          , { mapKey = "tools/cc/nixpkgs.nix", mapValue = "910583bcfa3664bdc3abef72d1bbc248d51f25ceb9e394bbe775b077b8789206" }
          , { mapKey = "tools/cc/nixpkgs_pin_test.sh", mapValue = "2b6d116888fa88ce9c4ed101ceddfbeb6f933b3940dde9f34083458458790fbc" }
          , { mapKey = "tools/cc/zig_cc.BUILD", mapValue = "93048670eac2e0e9a226627a762dc28ef86ee5cd223756398aa42d17fc30501f" }
          , { mapKey = "tools/cc/zig_cc.nix", mapValue = "f536da29f96dfce255ab24bb75999fbcf93d2196561c9aca5affa4ca7e7ab26d" }
          , { mapKey = "tools/clippy_test.sh", mapValue = "1a0c16f7e334bdfae84d8f25dab84cc2dfb45a8adcd95189e0d9662d10559261" }
          ]
      , image = "ghcr.io/xoxd-ai/agent-switchboard@sha256:4ebaf180547b5a74ca51489381270792d6d87619a4444eb9a7fcd6c31da94490"
      , live_acceptance = False
      , manifest_size = 4579
      , platform =
        { architecture = "amd64"
        , cmd = [ "serve" ]
        , entrypoint = [ "/usr/local/bin/swb" ]
        , os = "linux"
        , user = "65532"
        }
      , pr_heads =
          [ { mapKey = "28", mapValue = "a84b8ded026136c16588b570aa42b227ed9caca4" }
          ]
      , publication_receipt =
          "PUBLISHED 2026-10-13T01:04Z by the release workflow under the signed v0.2.2 tag (tag object b8fd62e66ecd884fefc77f9053cfbd77a9b22a22 on 911d643ec38b159e95a075c939b5cfd65a4870ec): run https://github.com/xoxd-ai/agent-switchboard/actions/runs/38327423160 rebuilt the approved digest, pushed it by digest with no registry tag (01:04:01Z-01:04:13Z) and read it back; release-check --registry-manifest passed (supplied_registry_manifest_matches true); registry readback digest sha256:4ebaf180547b5a74ca51489381270792d6d87619a4444eb9a7fcd6c31da94490, 4579 bytes, byte-identical to the approved manifest; evidence artifact release-evidence-v0.2.2 (id 11781494396, zip sha256:b377ade26bc859fbd317251be1401633ddb136a69cce731525fa2ce2570f527b, kept 90 days); anonymous readback from neo at 2026-10-13T01:29Z is byte-identical to that artifact; receipt TIN-5770 comment f8d42ee2-9e16-4184-a021-764d610131ed; ratified SWB-R59 under R-C435 (TIN-5770 comment d3d90700) applied under R-C655 (TIN-5770 comment be5a4566)"
      , qualification = "published-candidate-only"
      , rulings = [ "SWB-R59", "R-C416", "R-C435", "R-C655", "R-N13" ]
      , schema = "swb.approved-release.v1"
      , source = "daa6d59f41d43feeb60e65409203a109703df897"
      , source_inputs =
          [ ".bazelversion", "BUILD.bazel", "MODULE.bazel", "MODULE.bazel.lock", "Cargo.toml", "Cargo.lock", "cargo-bazel-lock.json", "crates/", "deploy/", "platforms/", "tools/" ]
      , upstream_main = "f29769b3bf698772ce644c9f23f7d7033ab75e5c"
      }
    : ApprovedRelease
