-- Explicit live acceptance only. IDs belong in the native place's StringValue,
-- not in this script, the converter, or credential-free CI.
local configuration = game:FindFirstChild("AcceptanceConfiguration")
assert(configuration and configuration:IsA("StringValue"), "Missing acceptance configuration")
local ids = game:GetService("HttpService"):JSONDecode(configuration.Value)
local results = {}
local failures = 0

local function check(name, operation)
    local ok, result = pcall(operation)
    table.insert(results, {name=name, passed=ok, result=if ok then result else tostring(result)})
    if not ok then failures += 1 end
end

local function preload(instance)
    local statuses = {}
    game:GetService("ContentProvider"):PreloadAsync({instance}, function(id, status)
        table.insert(statuses, {id=id, status=tostring(status)})
    end)

    -- Roblox catches errors inside its preload callback. Validate AFTER the call
    -- so a failed callback cannot be followed by a false PASS marker.
    assert(#statuses > 0, "Missing preload status")
    for _, result in statuses do
        assert(result.status == tostring(Enum.AssetFetchStatus.Success), result.id .. ": " .. result.status)
    end
    return statuses
end

check("staticMesh", function() return preload(workspace.Static) end)
check("skinnedMesh", function() return preload(workspace.Skin) end)

check("image", function()
    local image = Instance.new("ImageLabel")
    image.Image = "rbxassetid://" .. ids.image
    local statuses = preload(image)
    image:Destroy()
    return statuses
end)

check("audio", function()
    local sound = Instance.new("Sound")
    sound.SoundId = "rbxassetid://" .. ids.audio
    sound.Volume = 0
    sound.Parent = workspace
    preload(sound)
    assert(sound.IsLoaded, "Audio did not decode")
    assert(math.abs(sound.TimeLength - ids.audioDuration) < ids.audioTolerance, "Decoded audio duration differs")
    local duration = sound.TimeLength
    sound:Destroy()
    return {duration=duration}
end)

check("animation", function()
    local clip = game:GetService("KeyframeSequenceProvider"):GetKeyframeSequenceAsync("rbxassetid://" .. ids.animation)
    local frames = clip:GetKeyframes()
    assert(#frames == 2, "Keyframes lost")
    table.sort(frames, function(a, b) return a.Time < b.Time end)
    assert(frames[1].Time == 0 and frames[2].Time == 1, "Keyframe timing changed")
    assert(frames[2]:FindFirstChild("Root").CFrame.Position.Y == 1, "Pose transform changed")
    clip:Destroy()
    return {keyframes=2}
end)

check("model", function()
    local model = game:GetService("InsertService"):LoadAsset(tonumber(ids.model))
    local block = model:FindFirstChild("AcceptanceBlock", true)
    assert(block and block:IsA("Part"), "Model child lost")
    assert(block.Size == Vector3.new(4,6,8), "Model dimensions changed")
    model:Destroy()
    return {size="4,6,8"}
end)

print("ROBLOX_TOOLCHAIN_REMOTE_SUITE_RESULTS", game:GetService("HttpService"):JSONEncode({version=version(), failures=failures, results=results}))
assert(failures == 0, "Remote suite has failures")
print("ROBLOX_TOOLCHAIN_REMOTE_SUITE_PASS")
