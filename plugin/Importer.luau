--!strict
local AssetService = game:GetService("AssetService")
local RunService = game:GetService("RunService")
local ChangeHistoryService = game:GetService("ChangeHistoryService")

local Importer = {}
local PROJECT_ATTRIBUTE = "AssetLinkProject"
local ASSET_ATTRIBUTE = "AssetLinkAsset"
local REVISION_ATTRIBUTE = "AssetLinkRevision"
local retained: { [Model]: { EditableMesh } } = {}

local function vector(value: { number }): Vector3
	return Vector3.new(value[1], value[2], value[3])
end

local function srgb(linear: number): number
	return if linear <= 0.0031308 then linear * 12.92 else 1.055 * linear ^ (1 / 2.4) - 0.055
end

local function createPart(data: any): (MeshPart, EditableMesh)
	local editable = AssetService:CreateEditableMesh()
	assert(editable, "Studio could not allocate an EditableMesh")
	local part: MeshPart? = nil
	local ok, message = pcall(function()
		local vertices, normals, uvs = {}, {}, {}
		local minimum = Vector3.new(math.huge, math.huge, math.huge)
		local maximum = -minimum
		for _, position in data.vertices do
			local point = vector(position)
			minimum = minimum:Min(point)
			maximum = maximum:Max(point)
			table.insert(vertices, editable:AddVertex(point))
		end
		for _, normal in data.normals do
			table.insert(normals, editable:AddNormal(vector(normal)))
		end
		for _, uv in data.uvs do
			table.insert(uvs, editable:AddUV(Vector2.new(uv[1], uv[2])))
		end
		for _, triangle in data.triangles do
			local a, b, c = triangle[1] + 1, triangle[2] + 1, triangle[3] + 1
			local face = editable:AddTriangle(vertices[a], vertices[b], vertices[c])
			editable:SetFaceNormals(face, { normals[a], normals[b], normals[c] })
			editable:SetFaceUVs(face, { uvs[a], uvs[b], uvs[c] })
		end

		local settings = data.settings
		local meshPart = AssetService:CreateMeshPartAsync(Content.fromObject(editable), {
			CollisionFidelity = Enum.CollisionFidelity[settings.collisionFidelity],
		})
		part = meshPart
		meshPart.Name = data.name
		meshPart.Size = maximum - minimum
		meshPart.CFrame = CFrame.new((minimum + maximum) / 2)
		-- The GLB origin remains the pivot, including the door's hinge origin.
		meshPart.PivotOffset = meshPart.CFrame:Inverse()
		meshPart.Anchored = settings.anchored
		meshPart.CanCollide = settings.canCollide
		meshPart.CanTouch = settings.canTouch
		meshPart.CanQuery = settings.canQuery
		meshPart.DoubleSided = settings.doubleSided
		meshPart.Transparency = if type(settings.transparency) == "number"
			then settings.transparency
			else 1 - data.color[4]
		meshPart.Color = Color3.new(srgb(data.color[1]), srgb(data.color[2]), srgb(data.color[3]))
	end)
	if not ok then
		if part then
			part:Destroy()
		end
		editable:Destroy()
		error(message)
	end
	assert(part, "Mesh creation returned no part")
	return part, editable
end

local function discard(model: Model, meshes: { EditableMesh })
	model:Destroy()
	for _, mesh in meshes do
		mesh:Destroy()
	end
	retained[model] = nil
end

local function build(asset: any, projectId: string): (Model, { EditableMesh })
	local model = Instance.new("Model")
	model.Name = asset.name
	model:SetAttribute(PROJECT_ATTRIBUTE, projectId)
	model:SetAttribute(ASSET_ATTRIBUTE, asset.id)
	model:SetAttribute(REVISION_ATTRIBUTE, asset.revision)
	local meshes = {}
	local ok, message = pcall(function()
		for _, data in asset.meshes do
			local part, mesh = createPart(data)
			table.insert(meshes, mesh)
			part.Parent = model
		end
		model.WorldPivot = CFrame.identity
	end)
	if not ok then
		discard(model, meshes)
		error(`{asset.name}: {tostring(message)}`)
	end
	return model, meshes
end

function Importer.sync(snapshot: any): number
	assert(not RunService:IsRunning(), "Stop Play before syncing assets")
	assert(snapshot.protocol == 2, "Unsupported Studio adapter protocol")
	local config = snapshot.config
	local destination = game:GetService(config.destination)
	local folder: Folder? = nil
	-- Find only our direct managed container, even after a destination/name change.
	for _, serviceName in { "Workspace", "ServerStorage", "ReplicatedStorage" } do
		for _, child in game:GetService(serviceName):GetChildren() do
			if child:IsA("Folder") and child:GetAttribute(PROJECT_ATTRIBUTE) == config.projectId then
				assert(folder == nil, "Duplicate managed containers; resolve before syncing")
				folder = child
			end
		end
	end
	local collision = destination:FindFirstChild(config.containerName)
	assert(collision == nil or collision == folder, "Destination name belongs to an unmanaged instance")
	local existing: { [string]: Model } = {}
	if folder then
		for _, child in folder:GetChildren() do
			if child:IsA("Model") and child:GetAttribute(PROJECT_ATTRIBUTE) == config.projectId then
				local name = child:GetAttribute(ASSET_ATTRIBUTE)
				if type(name) == "string" then
					assert(existing[name] == nil, `Duplicate managed asset: {name}`)
					existing[name] = child
				end
			end
		end
	end

	local staged = {}
	local present = {}
	local ok, message = pcall(function()
		for _, asset in snapshot.assets do
			assert(type(asset.id) == "string" and asset.id ~= "", "Asset identity is missing")
			assert(not present[asset.id], `Duplicate asset: {asset.id}`)
			present[asset.id] = true
			local old = existing[asset.id]
			-- Rebuild on plugin reload, because live EditableMesh references need rehydration.
			if old and retained[old] and old:GetAttribute(REVISION_ATTRIBUTE) == asset.revision then
				continue
			end
			local model, meshes = build(asset, config.projectId)
			table.insert(staged, { model = model, meshes = meshes, old = old })
		end
		assert(not RunService:IsRunning(), "Play started while building; import cancelled")
	end)
	if not ok then
		for _, item in staged do
			discard(item.model, item.meshes)
		end
		error(message)
	end

	-- Nothing is replaced until the entire batch has built successfully.
	local recording = ChangeHistoryService:TryBeginRecording("AssetLinkSync", "Sync local assets")
	if not recording then
		for _, item in staged do
			discard(item.model, item.meshes)
		end
		error("Studio cannot begin an undo recording; sync cancelled")
	end
	if not folder then
		folder = Instance.new("Folder")
		folder:SetAttribute(PROJECT_ATTRIBUTE, config.projectId)
	end
	assert(folder, "Managed container could not be created")
	folder.Name = config.containerName
	folder.Parent = destination
	for _, item in staged do
		if item.old then
			item.model:PivotTo(item.old:GetPivot())
		end
		item.model.Parent = folder
		retained[item.model] = item.meshes
		if item.old then
			discard(item.old, retained[item.old] or {})
		end
	end
	for name, old in existing do
		if not present[name] then
			discard(old, retained[old] or {})
		end
	end
	ChangeHistoryService:FinishRecording(recording, Enum.FinishRecordingOperation.Commit)
	return #staged
end

return Importer
