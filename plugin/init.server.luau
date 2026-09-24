--!strict
local HttpService = game:GetService("HttpService")
local RunService = game:GetService("RunService")
local Importer = require(script.Importer)

local toolbar = plugin:CreateToolbar("Asset Link")
local toggle = toolbar:CreateButton("Asset Link", "Local GLB sync and project configuration", "")
local panel = plugin:CreateDockWidgetPluginGuiAsync(
	"AssetLinkSettings",
	DockWidgetPluginGuiInfo.new(Enum.InitialDockState.Right, false, false, 460, 650, 320, 400)
)
panel.Title = "Asset Link — local assets"

local frame = Instance.new("ScrollingFrame")
frame.Size = UDim2.fromScale(1, 1)
frame.CanvasSize = UDim2.fromScale(0, 0)
frame.AutomaticCanvasSize = Enum.AutomaticSize.Y
frame.Parent = panel
local layout = Instance.new("UIListLayout")
layout.Padding = UDim.new(0, 6)
layout.Parent = frame

local function textBox(placeholder: string, height: number): TextBox
	local box = Instance.new("TextBox")
	box.Size = UDim2.new(1, -12, 0, height)
	box.ClearTextOnFocus = false
	box.Text = ""
	box.PlaceholderText = placeholder
	box.TextSize = 14
	box.Font = Enum.Font.Code
	box.TextXAlignment = Enum.TextXAlignment.Left
	box.Parent = frame
	return box
end

local endpoint = textBox("Bridge URL printed by executable", 30)
endpoint.Text = plugin:GetSetting("Endpoint") or ""
local token = textBox("Session token printed by executable", 30)
local settings = textBox("Load config to edit the project's JSON file", 400)
settings.MultiLine = true
settings.TextYAlignment = Enum.TextYAlignment.Top
settings.TextWrapped = true

local status = Instance.new("TextLabel")
status.Size = UDim2.new(1, -12, 0, 60)
status.TextWrapped = true
status.Text = "Local preview only. No cloud upload."
status.TextSize = 14

local revision: string? = nil
local busy = false
local watching = false
local alive = true
local pollSeconds: number? = nil

local function request(method: string, path: string, body: any?): any
	assert(endpoint.Text:match("^http://127%.0%.0%.1:%d+$"), "Use the bridge's http://127.0.0.1:PORT URL")
	assert(token.Text ~= "", "Enter the session token")
	local response = HttpService:RequestAsync({
		Url = endpoint.Text .. path,
		Method = method,
		Headers = { Authorization = "Bearer " .. token.Text, ["Content-Type"] = "application/json" },
		Body = if body then HttpService:JSONEncode(body) else nil,
	})
	assert(response.Success, `HTTP {response.StatusCode}: {response.Body}`)
	return HttpService:JSONDecode(response.Body)
end

local function run(action: () -> ())
	if busy then
		return
	end
	busy = true
	local ok, message = pcall(action)
	busy = false
	if not ok then
		watching = false
		status.Text = tostring(message)
		warn("Asset Link: " .. tostring(message))
	end
end

local function button(label: string, action: () -> ())
	local control = Instance.new("TextButton")
	control.Size = UDim2.new(1, -12, 0, 30)
	control.Text = label
	control.TextSize = 14
	control.Parent = frame
	control.Activated:Connect(function()
		run(action)
	end)
end

local function displayConfig(response: any)
	revision = response.revision
	settings.Text = response.text
	pollSeconds = response.value.pollSeconds
end

local function sync()
	assert(not RunService:IsRunning(), "Stop Play before syncing")
	local snapshot = request("GET", "/snapshot", nil)
	local count = Importer.sync(snapshot)
	pollSeconds = snapshot.config.pollSeconds
	status.Text = `Synced {count} changed assets. Local preview; publishable assets are not created.`
end

button("Load configuration", function()
	displayConfig(request("GET", "/config", nil))
	plugin:SetSetting("Endpoint", endpoint.Text)
	status.Text = "Loaded the bridge's JSON config. Edit here or on disk."
end)
button("Apply configuration to file", function()
	assert(revision, "Load configuration first")
	displayConfig(request("PUT", "/config", { revision = revision, value = HttpService:JSONDecode(settings.Text) }))
	status.Text = "Configuration validated and saved."
end)
button("Sync now", sync)
button("Toggle live sync", function()
	if watching then
		watching = false
		status.Text = "Live sync stopped."
	else
		sync()
		watching = true
	end
end)
status.Parent = frame

toggle.Click:Connect(function()
	panel.Enabled = not panel.Enabled
end)
plugin.Unloading:Connect(function()
	alive = false
	watching = false
end)

task.spawn(function()
	while alive do
		if watching and not RunService:IsRunning() then
			run(sync)
		end
		-- Wait on a frame until the config supplies an interval, not a hidden policy default.
		if pollSeconds then
			task.wait(pollSeconds)
		else
			RunService.Heartbeat:Wait()
		end
	end
end)
