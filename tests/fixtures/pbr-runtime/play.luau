local rendering = settings():GetService("RenderSettings")
local previousQuality = rendering.QualityLevel
rendering.QualityLevel = Enum.QualityLevel.Level21

local client = Instance.new("LocalScript")
client.Name = "PBRVisualAcceptanceCamera"
client.Source = [[
    local camera = workspace.CurrentCamera
    camera.CameraType = Enum.CameraType.Scriptable
    camera.CFrame = CFrame.lookAt(Vector3.new(0,8,30),Vector3.new(0,3,0))
    camera.FieldOfView = 50

    for _, name in {"COLOR","NORMAL","ROUGH","METAL","ALL FOUR"} do
        local sphere = workspace:WaitForChild(name,30)
        assert(sphere,"Missing sphere " .. name)

        local gui = Instance.new("BillboardGui")
        gui.Size = UDim2.fromOffset(145,35)
        gui.StudsOffset = Vector3.new(0,3,0)
        gui.AlwaysOnTop = true

        local label = Instance.new("TextLabel")
        label.Size = UDim2.fromScale(1,1)
        label.Text = name
        label.TextSize = 20
        label.TextColor3 = Color3.new(1,1,1)
        label.BackgroundColor3 = Color3.new(0,0,0)
        label.Parent = gui
        gui.Parent = sphere
    end

    print("PBR_PUBLISHED_CLIENT_READY",game:GetService("RunService"):IsRunning())
]]
client.Parent = game:GetService("StarterPlayer"):WaitForChild("StarterPlayerScripts")

local server = Instance.new("Script")
server.Name = "PBRVisualAcceptanceDeadline"
server.Source = [[
    game:GetService("Players").CharacterAutoLoads = false
    task.wait(120)
    game:GetService("StudioTestService"):EndTest({
        running=game:GetService("RunService"):IsRunning(),
        completed=true,
    })
]]
server.Parent = game:GetService("ServerScriptService")

local ok, result = pcall(function()
    return game:GetService("StudioTestService"):ExecutePlayModeAsync({test="published-pbr-spheres"})
end)

client:Destroy()
server:Destroy()
rendering.QualityLevel = previousQuality

assert(ok,tostring(result))
assert(result.running and result.completed,"Play-mode fixture did not finish")
print("PBR_PUBLISHED_PLAY_RESULT",game:GetService("HttpService"):JSONEncode(result))
