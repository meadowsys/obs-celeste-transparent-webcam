local bit = require("bit")

local IGNORED_SOURCE_IDS = {
	"group",
	"coreaudio_input_capture"
}

local function ignore_source_id(type)
	for _, item in ipairs(IGNORED_SOURCE_IDS) do
		if item == type then return true end
	end

	return false
end

function script_description()
	return "Makes your webcam (or other source) transparent when Madeline Celeste enters its area, "
		.. "using data provided by Celeste Consistency Tracker"
		.. "\n\n"
		.. "This script adds a source to achieve this. todo the rest of the usage description"
end

local the = {}
the.id = "meadowsys-celeste-transparent-webcam"
the.output_flags = bit.bor(
	obslua.OBS_SOURCE_VIDEO,
	obslua.OBS_SOURCE_CUSTOM_DRAW
)
the.get_width = function() return 1920 end
the.get_height = function() return 1080 end

the.get_name = function()
	return "Celeste Transparent Webcam"
end

-- todo
the.get_properties = function(data)
	local props = obslua.obs_properties_create()

	local source_prop = obslua.obs_properties_add_list(
		props,
		"source",
		"Source",
		obslua.OBS_COMBO_TYPE_EDITABLE,
		obslua.OBS_COMBO_FORMAT_STRING
	)

	local sources = obslua.obs_enum_sources()
	if sources ~= nil then
		for _, source in ipairs(sources) do
			local source_id = obslua.obs_source_get_unversioned_id(source)
			if not ignore_source_id(source_id) then
				local name = obslua.obs_source_get_name(source)
				obslua.script_log(obslua.LOG_WARNING, "source name " .. name .. " id " .. source_id)
				obslua.obs_property_list_add_string(source_prop, name, name)
			end
		end
	end

	obslua.source_list_release(sources)
	-- source name (get list of sources etc)
	-- while there's no source selected, "if this does not automatically refresh, save and reopen the properties window"
	-- filter name
	-- x, y
	-- width, height
	-- use same thing as diagram in video https://youtu.be/I430pX9X3hM?si=Yr2lsavT8HgBUGpn&t=737

	-- todo test only
	-- obslua.obs_properties_add_bool(props, "testbool", "Test Bool")
	-- if data.bool_val then
	-- 	obslua.obs_properties_add_bool(props, "testbool2", "Test Bool 2")
	-- end

	-- source name 2-8 (get list of sources etc)

	-- todo data can be nil

	return props
end

-- todo
the.update = function(data, settings)
	-- todo test only
	-- data.bool_val = obslua.obs_data_get_bool(settings, "testbool")
	-- obslua.script_log(obslua.LOG_WARNING, "test log " .. tostring(data.bool_val))
end

-- todo
the.create = function(settings, source)
	local data = {}

	-- todo test only
	-- data.bool_val = obslua.obs_data_get_bool(settings, "testbool")

	return data
end

-- todo
-- the.destroy = function(data)
-- end

-- todo
-- the.video_render = function(data, effect)
-- end

obslua.obs_register_source(the)


-- function script_update(props)
-- 	obslua.script_log(obslua.LOG_WARNING, "test log")
-- end

-- create a custom source.
-- the source will have config options to choose another source to apply on
-- and the filter in the source
-- as well as the position and stuff
-- and then do it
-- maybe even a "draw rectangle" toggle to visualise it
