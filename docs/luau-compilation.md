# Local Luau compilation — issue 8

`roblox compile script.luau --config examples/compiler.json --output script.luauc`
uses the open-source Luau compiler linked through mlua. It never executes source,
starts Studio or sends scripts to Roblox. Invalid syntax fails before output
creation. Output is atomic, cannot overwrite an existing file, and reports both
source and bytecode hashes.

Compiler optimization, debug, type-info and coverage levels are explicit validated
JSON. Their bounded integer values are defined by the compiler API, not invented
execution policy. Dependencies and the Luau source revision are pinned by Cargo.lock
(currently luau0-src 0.15.11+luau697 through mlua 0.11.4).

Standalone bytecode is a local artifact: compatibility with Roblox's deployed
VM bytecode/version/security conventions is **not** claimed. Native scene assembly
uses compilation as a mandatory validation gate for supplied scripts, and packs
their original Source text, not this bytecode. Project type analysis, dependency
checking and actual engine tests remain separate unfinished work.
