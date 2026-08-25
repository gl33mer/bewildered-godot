shader_type canvas_item;

// Duotone gradient-map for the ink-stamp doodle atlas: dark luminance maps to
// the deep ink shadow, bright luminance to a dynamic gem/echo tint. The white
// card paper takes the highlight tint, ink strokes stay near-black — clean
// risograph-style color separation instead of a muddy modulate multiply.

uniform vec4 shadow_color : source_color = vec4(0.078, 0.078, 0.098, 1.0); // #141419
uniform vec4 highlight_color : source_color = vec4(0.0, 0.898, 1.0, 1.0); // Electric Cyan
uniform vec4 echo_color : source_color = vec4(1.0, 0.878, 0.51, 1.0); // Solar Gold #FFE082
uniform float echo_amount : hint_range(0.0, 1.0) = 0.0;
uniform float pulse_speed : hint_range(0.0, 10.0) = 2.4;
uniform float paper_tint : hint_range(0.0, 1.0) = 0.55; // how much highlight tints the paper

void fragment() {
	vec4 tex = texture(TEXTURE, UV);
	float lum = dot(tex.rgb, vec3(0.299, 0.587, 0.114));
	// Ink stays deep; paper picks up a softened highlight tint.
	vec3 ink_side = shadow_color.rgb;
	vec3 paper_side = mix(vec3(1.0), highlight_color.rgb, paper_tint);
	float shaped = smoothstep(0.08, 0.85, lum);
	vec3 duotone = mix(ink_side, paper_side, shaped);
	// Echo state: radiant solar gold with a subtle pulsing glow.
	float pulse = 0.5 + 0.5 * sin(TIME * pulse_speed);
	vec3 echo_tint = mix(echo_color.rgb, highlight_color.rgb, 0.18);
	vec3 paper_echo = mix(vec3(1.0), echo_tint, 0.45 + 0.25 * pulse);
	vec3 col = mix(duotone, mix(ink_side * 1.4, paper_echo, shaped), echo_amount);
	// Soft outer glow bleed on bright pixels while echoing.
	float glow = echo_amount * pulse * 0.35 * smoothstep(0.55, 1.0, lum);
	col += echo_tint * glow;
	COLOR = vec4(col, tex.a * COLOR.a);
}
