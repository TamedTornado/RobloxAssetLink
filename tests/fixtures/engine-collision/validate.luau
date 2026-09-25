-- Acceptance only: no source import, CreateMeshPartAsync, collision cooking or upload.
local results = {}
local tolerance = 0.02

local function near(actual, expected, label)
    assert(math.abs(actual - expected) < tolerance, label .. ": " .. actual .. " ~= " .. expected)
end

local function ray(part, x, z, expectedHeight)
    local parameters = RaycastParams.new()
    parameters.FilterType = Enum.RaycastFilterType.Include
    parameters.FilterDescendantsInstances = {part}
    local origin = part.CFrame:PointToWorldSpace(Vector3.new(x, 10, z))
    local direction = part.CFrame:VectorToWorldSpace(Vector3.new(0, -20, 0))
    local hit = workspace:Raycast(origin, direction, parameters)
    if expectedHeight == false then
        assert(not hit, part.Name .. " should have empty space at " .. x .. ", " .. z)
    else
        assert(hit, part.Name .. " expected collision at " .. x .. ", " .. z)
        near(part.CFrame:PointToObjectSpace(hit.Position).Y, expectedHeight, part.Name .. " surface")
    end
end

for _, name in {"Hull", "Box", "Decomposition"} do
    local part = workspace:FindFirstChild(name)
    assert(part and part:IsA("MeshPart"), "Missing native MeshPart " .. name)
    local density = part.CurrentPhysicalProperties.Density
    local expectedVolume = if name == "Box" then 64 elseif name == "Hull" then 64 / 6 else 128 / 6
    near(part:GetMass() / density, expectedVolume, name .. " volume from mass")
    part.Anchored = false
    part.CanCollide = false
    local center = if name == "Box" then Vector3.new(2, 2, 2) elseif name == "Hull" then Vector3.one else Vector3.new(5, 1, 1)
    local inertiaColumns = if name == "Box" then {
        Vector3.new(8 / 3, 0, 0), Vector3.new(0, 8 / 3, 0), Vector3.new(0, 0, 8 / 3),
    } else {
        Vector3.new(1.2, 0.2, 0.2),
        Vector3.new(0.2, if name == "Hull" then 1.2 else 17.2, 0.2),
        Vector3.new(0.2, 0.2, if name == "Hull" then 1.2 else 17.2),
    }

    for _, transform in {CFrame.identity, CFrame.new(20, 30, -40) * CFrame.Angles(0.3, 0.7, -0.2)} do
        part.CFrame = transform
        near((part.CFrame:PointToObjectSpace(part.AssemblyCenterOfMass) - center).Magnitude, 0, name .. " center of mass")
        for axis, unit in {Vector3.xAxis, Vector3.yAxis, Vector3.zAxis} do
            local torque = part:AngularAccelerationToTorque(part.CFrame:VectorToWorldSpace(unit), Vector3.zero)
            local localTorquePerMass = part.CFrame:VectorToObjectSpace(torque) / part:GetMass()
            near((localTorquePerMass - inertiaColumns[axis]).Magnitude, 0, name .. " inertia column " .. axis)
        end
        ray(part, 0.5, 0.5, if name == "Box" then 4 else 3)
        ray(part, 3, 3, if name == "Box" then 4 else false)
        if name == "Decomposition" then
            ray(part, 6, 0.5, false)
            ray(part, 8.5, 0.5, 3)
        end
    end

    table.insert(results, {name=name, mass=part:GetMass(), density=density, volume=expectedVolume})
end

print("ROBLOX_TOOLCHAIN_COLLISION_ACCEPTANCE_PASS", game:GetService("HttpService"):JSONEncode({version=version(), cases=results}))
