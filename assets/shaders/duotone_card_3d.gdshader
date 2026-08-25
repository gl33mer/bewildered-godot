shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_lambert, specular_disabled;

// Spatial twin of duotone_card.gdshader for 3D gem icon quads on the cube
// faces: luminance gradient-map from deep ink to a dynamic gem tint, with an
// optional solar-gold echo pulse.

uniform vec4 shadow_color : source_color = vec4(0.078, 0.078, 0.098, 1.0);
uniform vec4 highlight_color : source_color = vec4(0.0, 0.898, 1.0, 1.0);
uniform vec4 echo_color : source_color = vec4(1.0, 0.878, 0.51, 1.0);
uniform float echo_amount : hint_range(0.0, 1.0) = 0.0;
uniform float pulse_speed : hint_range(0.0, 10.0) = 2.4;
uniform float paper_tint : hint_range(0.0, 1.0) = 0.6;
// Atlas sub-region: xy = top-left offset (0..1), zw = size (0..1).
uniform vec4 atlas_region = vec4(0.0, 0.0, 1.0, 1.0);
uniform sampler2D source_tex : source_color, filter_linear_mipmap, repeat_disable;

void fragment() {
	vec2 uv = UV * atlas_region.zw + atlas_region.xy;
	vec4 tex = texture(source_tex, uv);
	float lum = dot(tex.rgb, vec3(0.299, 0.587, 0.114));
	vec3 ink_side = shadow_color.rgb;
	vec3 paper_side = mix(vec3(1.0), highlight_color.rgb, paper_tint);
	float shaped = smoothstep(0.08, 0.85, lum);
	vec3 duotone = mix(ink_side, paper_side, shaped);
	float pulse = 0.5 + 0.5 * sin(TIME * pulse_speed);
	vec3 echo_tint = mix(echo_color.rgb, highlight_color.rgb, 0.18);
	vec3 paper_echo = mix(vec3(1.0), echo_tint, 0.45 + 0.25 * pulse);
	vec3 col = mix(duotone, mix(ink_side * 1.4, paper_echo, shaped), echo_amount);
	float glow = echo_amount * pulse * 0.35 * smoothstep(0.55, 1.0, lum);
	col += echo_tint * glow;
	ALBEDO = col;
	EMISSION = echo_tint * echo_amount * pulse * 0.25 * smoothstep(0.5, 1.0, lum);
	ALPHA = tex.a;
}
