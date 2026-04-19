use agent_runner::context::repo_loader::RepoContentLoader;

#[test]
fn repo_loader_reads_pack_manifests_and_items() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pack_dir = temp.path().join("packs/base");
    std::fs::create_dir_all(&pack_dir).expect("create pack dir");
    std::fs::write(
        pack_dir.join("manifest.json"),
        r#"{
            "pack_id":"base",
            "version":1,
            "title":"DogBot Base Pack",
            "kind":"resource-pack",
            "source":{"source_id":"dogbot_local","repo_url":"local","ref":"workspace","license":"Proprietary"},
            "items":[
                {
                    "id":"base.system",
                    "kind":"prompt",
                    "path":"prompts/system.md",
                    "title":"System Prompt",
                    "summary":"base prompt",
                    "tags":["base"],
                    "enabled_by_default":true,
                    "platform_overrides":[],
                    "upstream_path":""
                }
            ]
        }"#,
    )
    .expect("write manifest");

    let loader = RepoContentLoader::new(temp.path().display().to_string());
    let packs = loader.load_packs().expect("load packs");

    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].pack_id, "base");
    assert_eq!(packs[0].items[0].id, "base.system");
}
