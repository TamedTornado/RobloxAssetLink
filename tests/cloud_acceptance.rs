use mlua::Lua;

#[test]
fn live_fixture_compiles_and_a_failed_preload_cannot_emit_pass() {
    let source = include_str!("fixtures/cloud-acceptance/validate.luau");
    mlua::Compiler::new().compile(source).unwrap();

    // Exercise the actual whole fixture with a fake engine, not Roblox/network.
    // All preload callbacks run behind pcall, matching the observed engine trap.
    let lua = Lua::new();
    lua.load(r#"
        Enum = {AssetFetchStatus={Success="Success"}}
        version = function() return "fake-engine" end
        workspace = {Static={}, Skin={}}
        Instance = {new=function() return {Destroy=function() end} end}
        local configuration = {Value="fixture",IsA=function() return true end}
        local http = {
            JSONDecode=function() return {image="1",audio="2",animation="3",model="4",audioDuration=0.2,audioTolerance=0.05} end,
            JSONEncode=function(_, result) suiteResult=result; return "results" end,
        }
        local content = {PreloadAsync=function(_, instances, callback)
            pcall(callback,"rbxassetid://1","Failure")
        end}
        game = {
            FindFirstChild=function() return configuration end,
            GetService=function(_,name)
                if name == "HttpService" then return http end
                if name == "ContentProvider" then return content end
                error("unavailable fake service")
            end,
        }
        emittedPass = false
        print = function(message)
            if message == "ROBLOX_TOOLCHAIN_REMOTE_SUITE_PASS" then emittedPass = true end
        end
    "#).exec().unwrap();

    let error = lua.load(source).exec().unwrap_err();
    assert!(error.to_string().contains("Remote suite has failures"));
    assert!(!lua.globals().get::<bool>("emittedPass").unwrap());
    let result: mlua::Table = lua.globals().get("suiteResult").unwrap();
    let checks: mlua::Table = result.get("results").unwrap();
    for index in [1, 2, 3, 4] {
        let check: mlua::Table = checks.get(index).unwrap();
        assert!(!check.get::<bool>("passed").unwrap());
        assert!(check.get::<String>("result").unwrap().contains("Failure"));
    }
}
