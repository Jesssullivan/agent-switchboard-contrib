-- Source of docs/releases/approved-broker.json. `just spec-dhall` (and the
-- flake check spec-dhall) renders this with dhall-to-json and fails unless it
-- is identical, after `jq -S`, to the committed JSON. Edit this file and the
-- JSON together; release-check.py keeps reading the JSON. SWB-R62 (TIN-6749 a7256598, R-C944, R-C435), R-C229.
let ApprovedRelease = ./ApprovedRelease.dhall

in    { file_sha256 =
          [ { mapKey = ".bazelversion", mapValue = "1b9487d55bea47fea50d226cc9c53bc548877ad2a318bac8fbc8b320f429e5c5" }
          , { mapKey = "BUILD.bazel", mapValue = "56e6cc630af92bd173f7d9031a4773af3d7b8bb14d7c92d3604b48bf34f31e3c" }
          , { mapKey = "Cargo.lock", mapValue = "54033c920c13b3cd2f2a69f1ef215814a43d0450adae58bf96a03d5c70d3b088" }
          , { mapKey = "Cargo.toml", mapValue = "6b822cd51bc7fc897e6dc42bf08eb0ae9bc225bd688865ca85528e07ea503cc1" }
          , { mapKey = "MODULE.bazel", mapValue = "fdb4822d8dfc5a90f8ce4fadcb37a84768c698319b98f61370425f742f841cf4" }
          , { mapKey = "MODULE.bazel.lock", mapValue = "be6ace0049afca35c99f7a57cc1647d9f392e9df4f2b814e4f5adbb0950c9b30" }
          , { mapKey = "cargo-bazel-lock.json", mapValue = "6be3c10bb32a4238e4a7be6740d681491a8d1536e1f0e7d291cd98bf9a4bc02c" }
          , { mapKey = "crates/swb-agentd/BUILD.bazel", mapValue = "4c53792c73662c8cf3737e36559acc042f53a28c4c5d638586e177df1f56adb7" }
          , { mapKey = "crates/swb-agentd/Cargo.toml", mapValue = "9a9241ecf6f202db1835a13ff1a7191be18e29283adecd0542ff0a5a2cb097e8" }
          , { mapKey = "crates/swb-agentd/src/lib.rs", mapValue = "03c8ff21db3b6556a4de5436ecf22589bf4ed43303cd6504b743394ad218bef7" }
          , { mapKey = "crates/swb-broker/BUILD.bazel", mapValue = "e2913c2d01c34aa7dd5571d9d8690f96989b412e8b53fffe77ed25f5727d96b8" }
          , { mapKey = "crates/swb-broker/Cargo.toml", mapValue = "b92e7edb6b245a708f197de03c8e644d166d7275084604eb2e67825122e21dc7" }
          , { mapKey = "crates/swb-broker/src/lib.rs", mapValue = "36e68c83d3e5a39f9ba79b58585c32638427e23737070abc9fee8994792c3ad4" }
          , { mapKey = "crates/swb-broker/src/service_roundtrip_tests.rs", mapValue = "1f390bfd1940b8ac24a091fd691d568331bc2bd630fc17631436a9b9c3dbc07a" }
          , { mapKey = "crates/swb-broker/tests/stamp.rs", mapValue = "f07fa0a5d40fbb81aa4062354d87f3baf216aa03f66b15520ce0bb6897b117f3" }
          , { mapKey = "crates/swb-proto/BUILD.bazel", mapValue = "25fcf691c3b25c02249fba3b5f880672b5da088a5566f9d8b9ab4076ae604e8c" }
          , { mapKey = "crates/swb-proto/Cargo.toml", mapValue = "b88b6f22fe6d6f6fc34aff96fa459628085e44cd1841f615dfc77b8925c01471" }
          , { mapKey = "crates/swb-proto/src/lib.rs", mapValue = "e1ec49c89bdb451dce973ac6b403487848afe06a22a2b81cd8de66da246228c9" }
          , { mapKey = "crates/swb-store/BUILD.bazel", mapValue = "e9a2c5ecf35dccb9cd8a7e6762fb4e9f6e411fec12aee6e7bab23dec1f83d178" }
          , { mapKey = "crates/swb-store/Cargo.toml", mapValue = "1c7dcf3a8476f2dbb4e6097a623326906753f919d00c8abea9722e25d332c459" }
          , { mapKey = "crates/swb-store/src/claims.rs", mapValue = "d19ef37184e1aac091ef978cd1ac11a7b7514283154af7077f3428ad606c1a89" }
          , { mapKey = "crates/swb-store/src/lib.rs", mapValue = "bedb3b9a650f8a33a9e9a0ea410275b6507b4714bd7c8489542ae2daafe60565" }
          , { mapKey = "crates/swb/BUILD.bazel", mapValue = "0dce5b88a9271cf234e307faeb64cacafa2b3eea865c216b8413b97a2b25fd61" }
          , { mapKey = "crates/swb/Cargo.toml", mapValue = "66eeb9a4fb4d22b4735f688bc8dfa359db870b23d744cd7af74895f79b71de35" }
          , { mapKey = "crates/swb/src/channel.rs", mapValue = "1c94dde4f31f2123c6d31208eb71cea03cc7dfac449b510dc291234c7f4d6f4d" }
          , { mapKey = "crates/swb/src/main.rs", mapValue = "cbe9bd9b79365b7c0da3a44728170d5168b8522ffb9ffeaef4451a835e0682c4" }
          , { mapKey = "crates/swb/tests/hook_blackhole_test.sh", mapValue = "7c6372a01c8d0cf5602188ad562ff77788c0dec0a2c9f3e202def88a7fa22ac3" }
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
      , image = "ghcr.io/xoxd-ai/agent-switchboard@sha256:2e42fc30cabdee4b076bd80d7d1e4b80080fc81436153210368daaa0caea7172"
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
          [ { mapKey = "43", mapValue = "572da8015920c816f9ba7c3440dfac0764a89c60" }
          ]
      , publication_receipt =
          "PUBLISHED 2026-10-23T02:25Z by the release workflow under the signed v0.3.0 tag (tag object 6b646390403a9475eb7890fdb5f95f0e1a649689 on ee1af0c4e2e2b260c57665f5feda24f2ec0dfe67): run https://github.com/xoxd-ai/agent-switchboard/actions/runs/39888543363 rebuilt the approved digest, pushed it by digest with no registry tag (02:25:15Z-02:25:28Z) and read it back; release-check --registry-manifest passed (supplied_registry_manifest_matches true); registry readback digest sha256:2e42fc30cabdee4b076bd80d7d1e4b80080fc81436153210368daaa0caea7172, 4579 bytes, byte-identical to the approved manifest; evidence artifact release-evidence-v0.3.0 (id 12474585352, zip sha256:4d045b921712cb514e443c6c52b6f6b6eafa50bc828c7f5d2549d51727899eeb, kept 90 days); anonymous readback from neo at 2026-10-23T02:29Z is byte-identical to that manifest; receipt TIN-6749 comment 6c3f1670-6fc6-4406-86ea-b8fd8abe0a67; ratified SWB-R62 under TIN-6749 comment a7256598 item 1 and R-C435 (TIN-5770 comment d3d90700)"
      , qualification = "published-candidate-only"
      , rulings = [ "SWB-R62", "R-C944", "R-C416", "R-C435", "R-N13" ]
      , schema = "swb.approved-release.v1"
      , source = "e2bb9f75c4a1c3db78f92945769b8fc2fa9bdd80"
      , source_inputs =
          [ ".bazelversion", "BUILD.bazel", "MODULE.bazel", "MODULE.bazel.lock", "Cargo.toml", "Cargo.lock", "cargo-bazel-lock.json", "crates/", "deploy/", "platforms/", "tools/" ]
      , upstream_main = "befba3015465a5df11a510a261b43f6b2980223c"
      }
    : ApprovedRelease
