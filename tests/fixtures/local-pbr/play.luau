-- Acceptance-only harness. Not injected into production game builds.
local client = Instance.new("LocalScript")
client.Source = [[
local service = game:GetService("AssetService")
local retained = {}
local camera = workspace.CurrentCamera
camera.CameraType = Enum.CameraType.Scriptable
camera.CFrame = CFrame.lookAt(Vector3.new(0,8,30),Vector3.new(0,3,0))
camera.FieldOfView = 50
for _, name in {"COLOR", "NORMAL", "ROUGH", "METAL", "ALL FOUR"} do
    local ok, problem = pcall(function()
        local maps = {}
        for property, file in {ColorMap="color", NormalMap="normal", RoughnessMap="roughness", MetalnessMap="metalness"} do
            local included = property == "ColorMap" or name == "ALL FOUR"
                or (property == "NormalMap" and name == "NORMAL")
                or (property == "RoughnessMap" and name == "ROUGH")
                or (property == "MetalnessMap" and name == "METAL")
            if included then
                local image = service:CreateEditableImageAsync(Content.fromUri("rbxasset://toolchain-local-pbr/" .. file .. ".png"))
                assert(image, "image allocation: " .. file)
                table.insert(retained, image)
                maps[property] = Content.fromObject(image)
            end
        end
        local surface = service:CreateSurfaceAppearanceAsync(maps)
        local sphere = workspace:WaitForChild(name)
        sphere:FindFirstChildOfClass("SurfaceAppearance"):Destroy()
        surface.Parent = sphere
        local gui = Instance.new("BillboardGui")
        gui.Size = UDim2.fromOffset(110,30)
        gui.StudsOffset = Vector3.new(0,3,0)
        gui.AlwaysOnTop = true
        local label = Instance.new("TextLabel")
        label.Size = UDim2.fromScale(1,1)
        label.Text = name
        label.TextSize = 16
        label.TextColor3 = Color3.new(1,1,1)
        label.BackgroundColor3 = Color3.new(0,0,0)
        label.Parent = gui
        gui.Parent = sphere
        print("LOCAL_PLAY_MATERIAL_OK", name)
    end)
    if not ok then warn("LOCAL_PLAY_MATERIAL_ERROR", name, problem) end
end
print("LOCAL_PLAY_MATERIAL_READY")
task.wait(90)
]]
client.Parent = game:GetService("StarterPlayer"):WaitForChild("StarterPlayerScripts")
local server = Instance.new("Script")
server.Source = [[
game:GetService("Players").CharacterAutoLoads = false
task.wait(90)
game:GetService("StudioTestService"):EndTest({completed=true})
]]
server.Parent = game:GetService("ServerScriptService")
print("LOCAL_PLAY_START")
local result = game:GetService("StudioTestService"):ExecutePlayModeAsync({test="local-material"})
print("LOCAL_PLAY_RESULT", game:GetService("HttpService"):JSONEncode(result))
