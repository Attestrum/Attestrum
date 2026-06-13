# Lookback corpus source — fineweb-edu sample-100BT

The fifth reference bundle (after WikiText-103, dolly-15k, PG-19, and
fineweb-edu sample-10BT, `fineweb10bt-corpus-source.md`) seals the
**sample-100BT** subset of **HuggingFaceFW/fineweb-edu**: 97,270,686 educational
web pages filtered from FineWeb (CommonCrawl-derived), 286.4 GB of compressed
parquet. This is the ladder's **headline 100 GB+ rung**. This note records
exactly which bytes are sealed so anyone can reproduce the build and verify they
started from the same input. **The corpus data itself is never committed to this
repository** — only this provenance record is.

It reuses the 10BT sharded architecture at 10× scale: the corpus plus its CAS
dwarfs any single runner's disk, so the `fineweb100bt-seal-crosscheck` workflow
seals it as a **140-job matrix** — one job per upstream parquet file — and an
`attestrum merge` job combines the 140 shard manifests into the single canonical
manifest + Merkle root. On the free plan the 140 jobs run ~20 at a time. The
merge uses the **streaming k-way merge** (commit `e71552c`): peak memory is
bounded by one Parquet row group plus the leaf-digest vector, independent of the
~97M total rows — this rung is the at-scale proof of that. The merged root is
byte-identical to what an unsharded build of the same rows would produce (the
sharding contract, `crates/attestrum-cli/tests/sharding.rs` +
`tests/merge_byte_identity.rs`). The `fineweb100bt-publish` workflow re-runs the
matrix and signs only if the canonical values reproduce.

## Source

| Field | Value |
|---|---|
| Hugging Face dataset | [`HuggingFaceFW/fineweb-edu`](https://huggingface.co/datasets/HuggingFaceFW/fineweb-edu) |
| HF revision (pinned) | `87f09149ef4734204d70ed1d046ddc9ca3f2b8f9` |
| Subset | `sample/100BT/` — 140 parquet files, 286,394,522,604 bytes total |
| Rows (= sealed leaves) | 97,270,686 |
| Columns | `text, id, dump, url, file_path, language, language_score, token_count, score, int_score` |
| License | `ODC-By-1.0` (dataset), subject to the CommonCrawl Terms of Use |

**One parquet row = one sealed leaf; the sealed bytes are the `text` column
bytes exactly** — no rendering, no normalization, no added newline (the PG-19
exact-bytes philosophy applied to a column). The metadata columns (`id`,
`dump`, `url`, scores, …) are **not** sealed; they remain available upstream.
The `source_uri` backref is the row's own `id` (a `urn:uuid`, globally unique
and shard-invariant); `source_dataset_id` is `fineweb-edu`; `language` is
carried from the row. The seal generator (`seal-fineweb-edu`) is reused
unchanged from the 10BT rung. See
`docs/diagrams/lookback/fineweb100bt-seal-pipeline.md`. The seal never touches
the protected fingerprint normalization (CLAUDE.md §4).

**Attribution.** The publish path renders a source / attribution / modification
section on the dataset card from
[`fineweb100bt-attribution.md`](./fineweb100bt-attribution.md) (passed verbatim
to `attestrum publish --attribution-file`).

## Pinned data files

The 140 parquet files under `sample/100BT/` at the revision above, with their
upstream LFS SHA-256 digests. Each matrix shard job downloads exactly one file
and asserts its digest **before sealing** — upstream byte drift fails the run
before anything is signed.

| File | Size (bytes) | SHA-256 |
|---|---|---|
| `000_00000.parquet` | 2153444469 | `955ff462f3db09a52a4750e7a69901f89d64e48dfc936de0847c9f234f32b695` |
| `000_00001.parquet` | 2152716615 | `776654388fdb2e5a1518b8d9d32543f418f1705d8bbaf408d221f08d76293680` |
| `000_00002.parquet` | 2152829440 | `85b0bc930150d24b44041745b47ecb1433d961cb6974a31227430644008eec49` |
| `000_00003.parquet` | 2151532316 | `34e77fe4ecc6075a2521d7716efb993636cd83ccefc2afc3fda3dbf35854c78b` |
| `000_00004.parquet` | 2151238351 | `2f3f66cd93f0cf4919725e6dbcd1c240314a2866c9527870231e6148061ca73f` |
| `000_00005.parquet` | 2152630983 | `9f089373c48cdeea728da9790bc369cc4d88243b3b18c13f5ec4e06e8446b6df` |
| `000_00006.parquet` | 2150872058 | `20e2491fc8dd6211998a550cdeb8a0e28b6d93b4b8495e463c248871c332acf1` |
| `000_00007.parquet` | 2150919214 | `98e15e6976fe0e9cafe4e695d37ca836cae9ffa3c4c3937f8f1325dc6bf899e8` |
| `000_00008.parquet` | 2151989277 | `0641091de585eea70376e19c25ca34ca913af321c715e5a40805219424bebdf4` |
| `000_00009.parquet` | 2150824077 | `6221c50fb09301b6d3653a18e384de1fe134c90e917926e987a502bcef91faf7` |
| `001_00000.parquet` | 2153082292 | `e2882e48990027b0a8866cf5a305ca7411c92c09ac465b7ec73ad5ccbed70cab` |
| `001_00001.parquet` | 2152454612 | `678e992f7b98482c5ded16da0b14d463550abeb5747340d5c5a8b6b7e410b526` |
| `001_00002.parquet` | 2153433162 | `8c1bb7f92e3199aa3a0a48ecf62946a3349af47772bde984cc83cca5bef267e6` |
| `001_00003.parquet` | 2153035941 | `f20dd1586eb6a9aab2eb8e97020d6c1e2b061fb88b9cee4103a0a98858023de5` |
| `001_00004.parquet` | 2151634678 | `cb3b26b79288c4a965ae8c99cfc93eba5e14c5ec98472120b7e1dedb0015c6f8` |
| `001_00005.parquet` | 2151681049 | `deae87e78f9811e23b835f59482734ba1d542f9bde91b93acf3bdd3902ede1c8` |
| `001_00006.parquet` | 2152289650 | `7f9ac4b60f7cfa292cba83a676ce3afedfae44d343363671acb215447d4af172` |
| `001_00007.parquet` | 2152010940 | `767f6546c88d78d630bd5ae6b091c82bec1026736c2d135fd331bfa161bd889f` |
| `001_00008.parquet` | 2152393119 | `4b58ba114b677c498d8ecece6798958d0eced3bb283f3f000feb278749f88cfb` |
| `001_00009.parquet` | 2152891815 | `5f3e20773432a85e89e4d7992b0b23e7f3ba107419fc17123436857bb1069fed` |
| `002_00000.parquet` | 2151897391 | `4f662867f4b7baefb017e25615fb53a91d521be914d330db6e96c19e3d8c1497` |
| `002_00001.parquet` | 2151671407 | `97c2b65c93604305f2ed862a448eddd2eb299402c75fbe7583852c2c1b3cac3b` |
| `002_00002.parquet` | 2151150989 | `65b07ff5f637f3ad2ac7f3cedc00c4e146e403f777f04d9854c8b981d15400f7` |
| `002_00003.parquet` | 2151434381 | `0c7ddaa8810808048c33ebfd2ef60e2be1598e73c1425dec36fc287cf7f72224` |
| `002_00004.parquet` | 2150751835 | `697a748496ba4fbfb4e94c01c8eb99a3ed8256b24a92307d87c9028c404c4822` |
| `002_00005.parquet` | 2152811934 | `3dee9accd7e7284eaa31b8a69aef426296d4231f94c0b56ba2f58178261d57e4` |
| `002_00006.parquet` | 2152081882 | `7726945362c39720377929d50afe53ca82c792f6c62545e1d2aedf86f09648ad` |
| `002_00007.parquet` | 2152237659 | `f93e5000c60bda7f5002003371ee6efe030cbf03a3c7c3bce1bcee19b4ce7a67` |
| `002_00008.parquet` | 2152594227 | `39eeed04750507860ea3c3f0713d39bc55aba1a70db02d22d24a414162d19e2b` |
| `002_00009.parquet` | 2151898537 | `16a04a934956c04595a54e45b47d93558e7f36ac619b5dc1ddb4909153d46ab8` |
| `003_00000.parquet` | 2151621817 | `3d510f630e04c7be663a5cf20db7a0ed86a75ffa4b0f455bae1cba42d351f1db` |
| `003_00001.parquet` | 2152532221 | `b60cf16e720a328deb6893a2065c76d0bc94413ac7d3aa81a0cd96f99821a0ae` |
| `003_00002.parquet` | 2152469007 | `bb40e3de81a83e212f75c8e29bee8158cce1f3188107b2c31a4b4d690bf01761` |
| `003_00003.parquet` | 2151776296 | `6608a6e81959b774e15e57e8c49c5155fa307052c45986e6a091824501860258` |
| `003_00004.parquet` | 2151543898 | `a82c8f5921078c75ad91fdde1939258440c5362c13ac98daeeb0280d1de2e243` |
| `003_00005.parquet` | 2151820572 | `3b1b1aa3e5eeae5b754a2566954d4fc0b172645396ba89e48de2487e64b41c8e` |
| `003_00006.parquet` | 2152409768 | `e0ac7e5d2a3c48efa7e6a431753ea61285022b272209ac284c4c25b2ccc9b9a6` |
| `003_00007.parquet` | 2151978423 | `72920f236f7e9ea7e01c1210d512240865b65551d4b50af0bd0f7ef5c4623ec4` |
| `003_00008.parquet` | 2150559471 | `e2168d2fdd28508e4d0e313a735c72fbf4a82c3907c4819c1a7cf9a113134b23` |
| `003_00009.parquet` | 2151098528 | `56aa32ba078320a9841b8c8bfec5485a0ee91d62f2358f23035c6f2b59e47e62` |
| `004_00000.parquet` | 2153002740 | `f49f1c354515d52827509821c188c1c782f7f342fe1807b878accccd3b6ea24d` |
| `004_00001.parquet` | 2153291928 | `910e38ed2219a9aa9396eba7b1d2ca28a744b73494b726de325bc012ef019e86` |
| `004_00002.parquet` | 2153053448 | `ea6b4982a9d08263a1a1f0864a2a39417e00c78ea59258fefb67374fe88e8f39` |
| `004_00003.parquet` | 2151470830 | `38a52b91572be74b2a1f95b03f967e433a4ada19755a0cf5986a9b3a11653690` |
| `004_00004.parquet` | 2153294237 | `e85605b286cbfef58e7e2e16f7f9b5676111f4c788d975ab02a3717bdb6afe67` |
| `004_00005.parquet` | 2152575246 | `b58df621355b14f956f19b1f948dd4ab3632c6456f5b4d3ef260365c9ae9f089` |
| `004_00006.parquet` | 2150848076 | `4d32c43836fa5025ba545aef9cf36611e264f862a68f5b58cede281392a1f0f8` |
| `004_00007.parquet` | 2152886435 | `ce56697450cf4f57edeadf7ee4d9f775008d7f2462a7a5ae1c169e76040fec61` |
| `004_00008.parquet` | 2151220422 | `61ba332e3fbe11332c9a010a2a0b64c83ed17df031b5178adca587dd798997c8` |
| `004_00009.parquet` | 2150892501 | `3c79a80adcb20ed955b7578083981adff986125bc2dd89e44ba0bad8cd618627` |
| `005_00000.parquet` | 2150708561 | `f84649e2cd775292f5a521a1095defb2e4b472d34e5c6c0030c50c0ee5fc018c` |
| `005_00001.parquet` | 2151500293 | `be9324f353455b39a22a79521f1b1c100a0c24c94f8275220039eac1aed920f6` |
| `005_00002.parquet` | 2150936398 | `bc43566bfe69c4fc0fe6115716c4cb6c7fa46e09dd527ea4b2a81b81b8ea3f8e` |
| `005_00003.parquet` | 2151166424 | `6ed97e6c8862c3502e1844e472ad16a9beb82e2c8e17ad5694e9a7045ff86be8` |
| `005_00004.parquet` | 2151637485 | `ba81670a4f8662a2f223b4669a0d3d1a6d7c883dfcd70494e8476b216117f078` |
| `005_00005.parquet` | 2153145072 | `0f870a2d57d287a6fe6b2f33d4385188c274ba8e4af4faf54a9494c401b01f23` |
| `005_00006.parquet` | 2152580193 | `6c379b4936cf25368212b370359090beb41e05b1fdbd1bd4ffa9942fae954b4c` |
| `005_00007.parquet` | 2153303408 | `09160cd46dc31123b2c49106196fa7fdc3a427ac0015f0089a41a4314f24efef` |
| `005_00008.parquet` | 2153302599 | `9f92cae38b7243ae306098c9f739fe0b2be56e2a36020eefd943882cac1dc3a3` |
| `005_00009.parquet` | 2152031536 | `ab3e2f0cddcae5f0d54eb0254cd859a0ed763be45fe433ed58830c24c98a8b62` |
| `006_00000.parquet` | 2151173759 | `ec3fe27193176de866775563aeb88827e90486dc97f811a85efea1e115c48cce` |
| `006_00001.parquet` | 2152105444 | `ec8686252960f21689025694ea7d1d825170a0b305998339b36ed1afe236c0a1` |
| `006_00002.parquet` | 2153360191 | `ae98b91c695a31af4930cda913d0f5a9fbf8778eb764034b3d67429eef2bfc0d` |
| `006_00003.parquet` | 2153401374 | `e7393b78a31aeccc3ac931622d5761ec9b22bc270c2942a862994cd7ee7e10c6` |
| `006_00004.parquet` | 2152464846 | `6eaa51d93913045c178e70d6e319595a02b7aad5b24703b5f290c313dfc945b5` |
| `006_00005.parquet` | 2150812800 | `2c9899beadfaeda7108c92f7b8c54115d74a12b280656fc9f937add178d30e6d` |
| `006_00006.parquet` | 2153385904 | `04f3b0996c65b2ffb99fd571a5409508b7e67df66f0fedb918b49aae9fee4778` |
| `006_00007.parquet` | 2153420855 | `2f7318492b187daf9e43a5d6928ad5ccd978affa487a59c54ec17018b795459e` |
| `006_00008.parquet` | 2152428323 | `0575dbd7247b5afc8b2866653fb9e89a1c8de35f6174c136aee76edb1c98050e` |
| `006_00009.parquet` | 2152932646 | `a8b3e8e5da7fedd316c76cdee79cd816cda48fddb6ab579949ec9059b181a4a7` |
| `007_00000.parquet` | 2153009115 | `861c1371876508fe9454c1fd039653a2b998029d13ab92747fe92ed30b82b487` |
| `007_00001.parquet` | 2152021786 | `7a3c3bcb55e4215a0e0720ea8e7de918007d4da0253789359922486e8bcda930` |
| `007_00002.parquet` | 2151133584 | `1c69052cc54c446e5ade6c6455038993a5f8321fd29015d5c33612467a17876d` |
| `007_00003.parquet` | 2151597441 | `de87e55703829ce55c49b9236d259e20e5c0813134a02cfb160167e32c785cf3` |
| `007_00004.parquet` | 2151524072 | `93777e43d9d00b508c8b061912378d09ff00cea6fa6fb71bb289bd03aa7423c3` |
| `007_00005.parquet` | 2150552642 | `c1354b2b762319b8c7ad56852910c909387f130713a65af862cef21a207aa9c4` |
| `007_00006.parquet` | 2153194366 | `4ced9db298b90aa92b3ad9207c3146bc79123381673ab4e8d68fdf3690b3693c` |
| `007_00007.parquet` | 2152609562 | `12500d5c20a47fc9fa013b069b83b13e479b2c3bef7e3ce39c84e8fc653199d3` |
| `007_00008.parquet` | 2151691660 | `9af6e214393faa7c91608569314d1c09367e1deeb26aa36485d1aa4753c5c3db` |
| `007_00009.parquet` | 2150969434 | `ad30fbf8bb1e0df31ea644c9e6941187414bdb0c59f21b6ff4585e78451ee78e` |
| `008_00000.parquet` | 2151138399 | `da61fb9e93618c8ed2a3e20329cf26ba7073ac797bfe8dbf523421dfa30e1b68` |
| `008_00001.parquet` | 2150598151 | `5846e99c57c716a364a48d8ddaf4b9ede53b52865e0ea2d9e4626a0ba1d749ed` |
| `008_00002.parquet` | 2151279431 | `6102754c22f5ba224b16633774523612727878ed42e1b364a8dcbb252a407a81` |
| `008_00003.parquet` | 2150642725 | `18294cbd02455e5b6c111d7510866f41f8073259ba4222037a0088654acccf19` |
| `008_00004.parquet` | 2150737656 | `ae6062d438564f32b3f674a083292963f5a430c9948c1043376629e4555ab4d0` |
| `008_00005.parquet` | 2152582259 | `5951a56ebd9edb291cb7b3ede8c0298e6d928b86b64e3159c67cc6373128e7f9` |
| `008_00006.parquet` | 2152453034 | `8906dd1ff192f5a77373f157b8e0daa590ec21797d34a92e2818c5c7284e31df` |
| `008_00007.parquet` | 2152831030 | `23012c54073f1362674e437b1c5ebe7c2bf998fdadb7adb4611e183dd17d1377` |
| `008_00008.parquet` | 2152346868 | `430a269d951db31c5ee389e2a6446591de13614f627c9b218fc209e7dc962a3c` |
| `008_00009.parquet` | 2151644920 | `29c4e9699b75fdc869988da0e75c17bfdfbec1df4e27102e40b23beaf7b11d00` |
| `009_00000.parquet` | 2152870180 | `8bc9e6ec942e9797fbc3a752f724428da4e400559df4c8752e3fc08b8d4fcc1a` |
| `009_00001.parquet` | 2150826267 | `2a92e5f0a71d0c09681c0378c57fd01f93d15804408fe48b9ef7b360400ad155` |
| `009_00002.parquet` | 2152785235 | `fd98882fd0c6f0715b519f73088e025399baaed5d47d23b69711230230ebb0b6` |
| `009_00003.parquet` | 2152793292 | `461b2907254965b904e15da071825e2c2f3627ba3d9b701376d8da2cf84f87c5` |
| `009_00004.parquet` | 2150735027 | `dfa31314933e7e925f1ef152f46452b3f086c2857a61528f1385251c2350ef6f` |
| `009_00005.parquet` | 2152442675 | `c3b30571f66e2a829a35d7d4ab132b6183ca30486c9cd2400430ae1e32871266` |
| `009_00006.parquet` | 2151220022 | `7759e684f61d43a2eb46312b189c2e6b98cfb9a2e4b001711342cc7e10a270a9` |
| `009_00007.parquet` | 2150434934 | `37b0026523ac5f0af08975a315e05be900587ba6ea129b16aaed82f968ff3102` |
| `009_00008.parquet` | 2150413813 | `58803123013d10a25d172cc6319d1606f5b52046672b425ece49b28fadca3167` |
| `009_00009.parquet` | 2150747289 | `f350a772442df44c87446af257c140066f0431c13759faddc9fd0285964b0a17` |
| `010_00000.parquet` | 2151571678 | `4fce79a4201b4b04e28030f0b7f677eb99084ca064178ebf050c920d50b5b424` |
| `010_00001.parquet` | 2151236248 | `8bc467f4fa0c96425675d0805d347c49440a1d4cbf3144ccf3ad5efb9fcd164f` |
| `010_00002.parquet` | 2150569557 | `5b21a98b61a2224f310cffd00773b334de2401b198bafe006b6251ee374c61a2` |
| `010_00003.parquet` | 2150341172 | `3d989ada06f5bc96fba4f1f376e2e6898f8dc92bda738cdb729f25ee8d88adfe` |
| `010_00004.parquet` | 2152703186 | `3ba6424a8797f9d90d9f629143ea9ff6abc1fd331ecaf20597848db7cecdacd6` |
| `010_00005.parquet` | 2151197012 | `03e1da7827b10a796486339220140c44087f5fea57b0e2f2d27436ef35182639` |
| `010_00006.parquet` | 2150869474 | `ed99e9b6f25a017b4bd002fd2470086ea5a729cbe1b364550f3808f08291c08b` |
| `010_00007.parquet` | 2152353010 | `04074548c93fce0dcd56a1c1eeccfd2ed28b96eae6b95bae96712e1cb2f30e73` |
| `010_00008.parquet` | 2150821210 | `5959b241b3825595e4817724c5d57b0f420a93642625c2022e13973014f062bc` |
| `010_00009.parquet` | 2151745378 | `5a49a432c677ad4deea070b9ba76b811be9c3cebc4e0c5e8974b8fcb928f33e0` |
| `011_00000.parquet` | 2150983433 | `69119d14738751f9182db6fd87a7582e10f33b9e35571c8205bf6facf6b0a42d` |
| `011_00001.parquet` | 2151833168 | `a8b8b5a3326d7ad5df554b4b2d6fb4401acbf7cc44b3894b677a384331e18b7a` |
| `011_00002.parquet` | 2150635926 | `1ca4d7ca7f5fb97f326e7cb3ffa63317efa6acb695553868f5a48882d939cbdc` |
| `011_00003.parquet` | 2152670300 | `ff5212cacd088f4127adc2041e18dcc170a8c2cb19dc92271ad22adaeed7e827` |
| `011_00004.parquet` | 2151487097 | `26b8429f5c0c6e561310eac9050b27ffc4ea0869980162b058b3ef38a06ebd7a` |
| `011_00005.parquet` | 2151541011 | `3c1708ff71a5fd15a8e62f2a591d60d5cc98c1f407e0a44e99011837154d2f39` |
| `011_00006.parquet` | 2152614900 | `1ccd8d87762291d10e0a4c67f599faebdd6f4dbb70398c7631f6d4b0a8d7d3bd` |
| `011_00007.parquet` | 2152992454 | `09083d8b1758a6e425029f3a370d11b78ac4ed5ad802d0b87b0ad353dd5cafbf` |
| `011_00008.parquet` | 2153390140 | `38318ff88ddfb4be048bce2490b19d34bd8225f070e594656c16c30088925bc2` |
| `011_00009.parquet` | 2152799518 | `9ba8e1241e041c4660d06936f38a616defe8326cc861b1aa9731125b85cefb15` |
| `012_00000.parquet` | 2151175692 | `5c98fd3f113deff5395deb9f958740977982a977920b32bd46129376088d642a` |
| `012_00001.parquet` | 2152085272 | `e81dbeb36b5ed534d49ade8587109105032574d5ace90d43a0d104cb37385e4a` |
| `012_00002.parquet` | 2150577007 | `63825137c8919213efe76b648a6461a80f34f27d8e0586f178d117b1b586e90a` |
| `012_00003.parquet` | 2153309654 | `5c9a0fb69c16a8a20fb8c76c619064734d2c704048349f96746fab2d95aeb26a` |
| `012_00004.parquet` | 2152431260 | `ade4b1a8e79f458be18d086cd2861b05605b77f9b677908b3848ec3052bfb68a` |
| `012_00005.parquet` | 2153266763 | `08043264388b9025fcd4abada9f4f03ac230e2b9f57b20f7f422c725a88b3d77` |
| `012_00006.parquet` | 2150459731 | `9dbdc83a49b85e44fb6a1255a29c8aeeca1ef4cd3882f868f549fb35cafe5a52` |
| `012_00007.parquet` | 2151129707 | `425cb44998932d9ac6ffbb039dc5fc6adf10aa5fa523c020a968e172e4095630` |
| `012_00008.parquet` | 2150448824 | `dda63bc89a0afb6f9d2883ab798301bf91c9b572ed17fe504deef7c4086acece` |
| `012_00009.parquet` | 2151070619 | `aad3c61ef272ec461e41bb40b8c6c1ebbb0a4b026808765f29fda04ce6fd9701` |
| `013_00000.parquet` | 730198774 | `5c618a6b5c584d857202542092c45633d8b98c7beeee4f54cad9bdd4c971a3df` |
| `013_00001.parquet` | 743123195 | `487da1a3e004fb2ca87996ca9836c16a49f64e8d70018944c819e49a62774e3a` |
| `013_00002.parquet` | 720295268 | `746fb74dac5227d1eb8b84c71ed05bf8952a25118575ebf2bf6d40b63c908326` |
| `013_00003.parquet` | 702232373 | `b0d7bf2012721323dc0ee2b86ce1aff5f02cfb4aab3f7cc0eb1510cb9e5322f0` |
| `013_00004.parquet` | 705597730 | `5407e63a718207c8f5e3982b3ef0d0835e7177e34d434fcb79234afd42532826` |
| `013_00005.parquet` | 692526551 | `b1b5b41b65cafff9f31190d1d487a3ec39c41e6e4698db6759a948f7a695b904` |
| `013_00006.parquet` | 758099222 | `2fbe83d800ab6c5a26f33122e342fac40135675708a71e6a280bcdad68293cc6` |
| `013_00007.parquet` | 209335613 | `9c74754bd8b524c04a6d2a57f1654340f8deb1414846db3456b83fe0e321f591` |
| `013_00008.parquet` | 673400814 | `cb27cb812ab0a0120823a05e67a5d5b0610dcf946782d381c8608f70cf858565` |
| `013_00009.parquet` | 710997519 | `92f097534c4887400ddea2f4878850c91f2b52c48bfc52354de61749c92e547a` |

## Local layout (gitignored — never committed)

```
_lookback-data/fineweb-edu-100bt/000_00000.parquet … 013_00009.parquet
```

Covered by the `.gitignore` tier-3 `/_*` glob (CLAUDE.md §0.5.5).

## Re-fetch + verify (one shard or all)

The 140 files are named `<group>_<part>.parquet` for groups `000`–`013` and
parts `00000`–`00009`:

```bash
rev=87f09149ef4734204d70ed1d046ddc9ca3f2b8f9
base=https://huggingface.co/datasets/HuggingFaceFW/fineweb-edu/resolve/$rev/sample/100BT
mkdir -p _lookback-data/fineweb-edu-100bt && cd _lookback-data/fineweb-edu-100bt
for g in $(seq 0 13); do for p in $(seq 0 9); do
  name="$(printf '%03d_%05d.parquet' "$g" "$p")"
  curl -sSL -o "$name" "$base/$name"
done; done
shasum -a 256 *.parquet            # must match the table above
```

The sealed shard manifests are then produced by the seal generator (per shard
or over the whole directory — the merged root is identical either way):

```bash
cargo run -p attestrum-pipeline --release --example seal-fineweb-edu \
  -- _lookback-data/fineweb-edu-100bt _lookback-fineweb100bt-out
```

## Canonical seal (input → output, closeable)

Sealing the pinned input above through the 140-shard matrix + streaming
`attestrum merge` yields this canonical result. A verifier who re-runs the
generator on the byte-identical input — sharded any way, or unsharded — must
reproduce the same Merkle root (multiset invariance;
`crates/attestrum-cli/tests/sharding.rs` + `tests/merge_byte_identity.rs`).
Captured by `fineweb100bt-seal-crosscheck` `mode=capture`
([run 27451315236](https://github.com/Attestrum/Attestrum/actions/runs/27451315236))
on Linux x86_64/glibc, the signing platform.

| Field | Value |
|---|---|
| Merkle root (BLAKE3, RFC 6962), merged | `9ded6e9d6174c03851ec1e2d060cbf81fffdd1c3b2c0ab41bcb4f9b70bfdeafe` |
| Leaves (rows) | 97,270,686 |
| Merged `manifest.parquet` SHA-256 | `939f9fda83f47723714faedde09de269e5a9c9a38fb84a335bf54cb5215b85f3` |
| Sealed by | `attestrum-pipeline` example `seal-fineweb-edu` (release, CI, 140-shard matrix) + streaming `attestrum merge` |

## Scale evidence (measured, capture run 27451315236)

The ladder's headline rung and the at-scale validation of the streaming merge.
Measured on a free standard GitHub Actions runner (ubuntu-24.04, 4 vCPU /
16 GB RAM):

| Metric | Measured |
|---|---|
| Matrix | 140 shard jobs, ~12 at a time (`max-parallel: 12`) |
| Merge (97,270,686 rows from 140 manifests) | wall **4:58**, peak RSS **9.18 GiB** (9,627,096 kB) |
| Merged `manifest.parquet` | ~6.9 GB |

**The streaming-merge result:** at 97.27M rows the previous load-everything
merge would have needed ~91 GiB (~970 B/row) — impossible on a 16 GB runner.
The streaming merge held **9.18 GiB** and fit with ~7 GB to spare, which is
what makes this rung feasible in free CI at all. That 9.18 GiB is higher than
the early ~4–5 GiB estimate: ~3.1 GB is the leaf-digest vector (97.27M × 32 B —
the one allocation that scales with rows, removable only by a streaming Merkle
root, which would touch §4 `attestrum-merkle`), and the remainder is the 140
shard readers held open simultaneously to seed the k-way heap, each carrying
Parquet decode buffers. For this rung that fits comfortably; rungs with many
more shards would want a smaller reader batch size or lazy reader open. The
merged bytes are byte-identical regardless.

**Download note:** a first capture attempt (run 27449603824, ~20-wide) had 6
shards fail on HTTP 429 (HF rate limiting); capping to 12-wide with hardened
retry (`--retry 10 --retry-delay 15 --retry-all-errors`) and re-running the 5
stragglers cleared it, and the merge produced the triple above.

## Published (signed + live)

> **PENDING an explicit founder go to dispatch `fineweb100bt-publish`** (§A9 —
> each publish is gated). Once published: HF dataset
> `Attestrum/fineweb-edu-sample-100BT-sealed`, predicate type
> `https://attestrum.com/attestation/training-corpus/v0.3`, Rekor logIndex, and
> the Attestrum GHA workflow signing identity are recorded here. The ~8 GB
> merged manifest exceeds cosign's 128 MiB blob-read cap, so verification uses
> the `--digest`/`--digestAlg` form (same as 10BT).
