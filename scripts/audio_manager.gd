extends Node

class_name AudioManagerScript

signal master_mute_changed(muted: bool)
signal music_mute_changed(muted: bool)
signal sfx_mute_changed(muted: bool)
signal master_volume_changed(volume: float)
signal music_volume_changed(volume: float)
signal sfx_volume_changed(volume: float)

## Audio bus indices (must match default_bus_layout.tres)
const BUS_MASTER: int = 0
const BUS_MUSIC: int = 1
const BUS_SFX: int = 2

## Pool of AudioStreamPlayers for overlapping SFX
var sfx_pool: Array[AudioStreamPlayer] = []
const SFX_POOL_SIZE: int = 16

## Cached audio streams
var swap_sound: AudioStreamWAV
var reject_sound: AudioStreamWAV
var match_chime_sound: AudioStreamWAV
var echo_detonate_sound: AudioStreamWAV
var special_create_sound: AudioStreamWAV
var music_track: AudioStreamWAV

## Volume and mute state (persisted)
var _master_mute: bool = false
var _music_mute: bool = false
var _sfx_mute: bool = false
var _master_volume: float = 1.0
var _music_volume: float = 0.7
var _sfx_volume: float = 1.0

func _ready() -> void:
	_init_audio_buses()
	_init_sfx_pool()
	_load_audio_assets()
	_apply_all_volumes()

func _init_audio_buses() -> void:
	# Ensure buses exist with correct names
	if AudioServer.get_bus_count() < 3:
		AudioServer.add_bus(BUS_MUSIC)
		AudioServer.add_bus(BUS_SFX)
	
	AudioServer.set_bus_name(BUS_MASTER, "Master")
	AudioServer.set_bus_name(BUS_MUSIC, "Music")
	AudioServer.set_bus_name(BUS_SFX, "SFX")
	
	# Set initial volumes
	AudioServer.set_bus_volume_db(BUS_MASTER, linear_to_db(_master_volume))
	AudioServer.set_bus_volume_db(BUS_MUSIC, linear_to_db(_music_volume))
	AudioServer.set_bus_volume_db(BUS_SFX, linear_to_db(_sfx_volume))

func _init_sfx_pool() -> void:
	for i in range(SFX_POOL_SIZE):
		var player = AudioStreamPlayer.new()
		player.bus = "SFX"
		add_child(player)
		sfx_pool.append(player)

func _load_audio_assets() -> void:
	# Procedurally generate clean placeholder sounds
	swap_sound = _generate_tone(440, 0.08, 0.3)      # A4 - swap swoosh
	reject_sound = _generate_tone(150, 0.15, 0.4)     # Low rejection thud
	match_chime_sound = _generate_tone(880, 0.12, 0.5) # A5 - match chime
	echo_detonate_sound = _generate_tone(100, 0.3, 0.8) # Deep boom
	special_create_sound = _generate_chord([660, 880, 1320], 0.2, 0.4) # E5, A5, E6 - sparkle
	music_track = _generate_ambient_loop()              # Ambient loop
	
	print("Audio assets generated")

func _generate_tone(freq: float, duration: float, volume: float) -> AudioStreamWAV:
	"""Generate a clean sine wave tone with quick envelope"""
	var sample_rate: int = 44100
	var samples: int = int(sample_rate * duration)
	var data = PackedFloat32Array()
	data.resize(samples)
	
	var attack_samples: int = int(sample_rate * 0.01)
	var release_samples: int = int(sample_rate * 0.05)
	
	for i in range(samples):
		var t: float = i / sample_rate
		var envelope: float = 1.0
		
		# Quick attack
		if i < attack_samples:
			envelope = i / attack_samples
		# Release
		elif i >= samples - release_samples:
			envelope = (samples - i) / release_samples
		
		var sample: float = sin(2.0 * PI * freq * t) * envelope * volume
		data.set(i, sample)
	
	var wav = AudioStreamWAV.new()
	wav.data = _pack_float32_array(data)
	wav.mix_rate = sample_rate
	wav.format = AudioStreamWAV.FORMAT_16_BITS
	wav.loop_mode = AudioStreamWAV.LOOP_DISABLED
	return wav

func _generate_chord(freqs: Array[float], duration: float, volume: float) -> AudioStreamWAV:
	"""Generate multiple frequencies mixed together"""
	var sample_rate: int = 44100
	var samples: int = int(sample_rate * duration)
	var data = PackedFloat32Array()
	data.resize(samples)
	
	var attack_samples: int = int(sample_rate * 0.01)
	var release_samples: int = int(sample_rate * 0.1)
	var freq_count: float = freqs.size()
	
	for i in range(samples):
		var t: float = i / sample_rate
		var envelope: float = 1.0
		
		if i < attack_samples:
			envelope = i / attack_samples
		elif i >= samples - release_samples:
			envelope = (samples - i) / release_samples
		
		var sample: float = 0.0
		for freq in freqs:
			sample += sin(2.0 * PI * freq * t) / freq_count
		
		data.set(i, sample * envelope * volume)
	
	var wav = AudioStreamWAV.new()
	wav.data = _pack_float32_array(data)
	wav.mix_rate = sample_rate
	wav.format = AudioStreamWAV.FORMAT_16_BITS
	wav.loop_mode = AudioStreamWAV.LOOP_DISABLED
	return wav

func _generate_ambient_loop() -> AudioStreamWAV:
	"""Generate a simple ambient music loop"""
	var sample_rate: int = 44100
	var duration: float = 8.0  # 8 second loop
	var samples: int = int(sample_rate * duration)
	var data = PackedFloat32Array()
	data.resize(samples)
	
	# Simple chord progression: Am - F - C - G (looped)
	var chords = [
		[220.0, 330.0, 440.0],   # A3, E4, A4 (Am)
		[174.6, 261.6, 349.2],   # F3, C4, F4 (F)
		[130.8, 261.6, 392.0],   # C3, C4, G4 (C)
		[196.0, 293.7, 392.0],   # G3, D4, G4 (G)
	]
	var chord_duration: float = duration / chords.size()
	var chord_samples: int = int(sample_rate * chord_duration)
	
	for chord_idx in range(chords.size()):
		var freqs = chords[chord_idx]
		var start: int = chord_idx * chord_samples
		var end: int = min(start + chord_samples, samples)
		
		for i in range(start, end):
			var t: float = (i - start) / sample_rate
			var sample: float = 0.0
			
			# Fade in/out each chord
			var local_progress: float = (i - start) / chord_samples
			var envelope: float = 1.0
			if local_progress < 0.05:
				envelope = local_progress / 0.05
			elif local_progress > 0.95:
				envelope = (1.0 - local_progress) / 0.05
			
			for freq in freqs:
				data[i] += sin(2.0 * PI * freq * t) / freqs.size()
			
			data[i] *= envelope * 0.15  # Low volume for ambient
	
	var wav = AudioStreamWAV.new()
	wav.data = _pack_float32_array(data)
	wav.mix_rate = sample_rate
	wav.format = AudioStreamWAV.FORMAT_16_BITS
	wav.loop_mode = AudioStreamWAV.LOOP_FORWARD
	return wav

func _pack_float32_array(data: PackedFloat32Array) -> PackedByteArray:
	"""Convert float32 samples to 16-bit PCM bytes"""
	var bytes = PackedByteArray()
	bytes.resize(data.size() * 2)
	for i in range(data.size()):
		var sample: int = int(clamp(data[i] * 32767.0, -32768, 32767))
		bytes.set(i * 2, sample & 0xFF)
		bytes.set(i * 2 + 1, (sample >> 8) & 0xFF)
	return bytes

func linear_to_db(linear: float) -> float:
	if linear <= 0.0:
		return -80.0
	return 20.0 * log(linear) / log(10.0)

func db_to_linear(db: float) -> float:
	if db <= -80.0:
		return 0.0
	return pow(10.0, db / 20.0)

# ===== Public API =====

func play_swap() -> void:
	_play_sfx(swap_sound, 1.0)

func play_reject() -> void:
	_play_sfx(reject_sound, 1.0)

func play_match(cascade_depth: int) -> void:
	var pitch: float = clamp(1.0 + (cascade_depth - 1) * 0.12, 1.0, 2.5)
	_play_sfx(match_chime_sound, pitch)

func play_echo_detonate() -> void:
	_play_sfx(echo_detonate_sound, 1.0)

func play_special_create() -> void:
	_play_sfx(special_create_sound, 1.0)

func _play_sfx(stream: AudioStreamWAV, pitch_scale: float) -> void:
	if _sfx_mute:
		return
	
	# Find available player in pool
	for player in sfx_pool:
		if not player.playing:
			player.stream = stream
			player.pitch_scale = pitch_scale
			player.volume_db = linear_to_db(_sfx_volume)
			player.play()
			return
	
	# All busy - steal oldest (shouldn't happen with adequate pool)
	var oldest = sfx_pool[0]
	oldest.stop()
	oldest.stream = stream
	oldest.pitch_scale = pitch_scale
	oldest.volume_db = linear_to_db(_sfx_volume)
	oldest.play()

func start_music() -> void:
	if _music_mute or music_track == null:
		return
	
	# Use a dedicated music player
	var player = AudioStreamPlayer.new()
	player.name = "MusicPlayer"
	player.bus = "Music"
	player.stream = music_track
	player.volume_db = linear_to_db(_music_volume)
	player.autoplay = true
	add_child(player)
	player.play()

func stop_music() -> void:
	for child in get_children():
		if child is AudioStreamPlayer and child.bus == "Music":
			child.stop()
			child.queue_free()

# ===== Volume/Mute Controls =====

func set_master_mute(muted: bool) -> void:
	_master_mute = muted
	AudioServer.set_bus_mute(BUS_MASTER, muted)
	emit_signal("master_mute_changed", muted)

func set_music_mute(muted: bool) -> void:
	_music_mute = muted
	AudioServer.set_bus_mute(BUS_MUSIC, muted)
	emit_signal("music_mute_changed", muted)

func set_sfx_mute(muted: bool) -> void:
	_sfx_mute = muted
	AudioServer.set_bus_mute(BUS_SFX, muted)
	emit_signal("sfx_mute_changed", muted)

func set_master_volume(volume: float) -> void:
	_master_volume = clamp(volume, 0.0, 1.0)
	AudioServer.set_bus_volume_db(BUS_MASTER, linear_to_db(_master_volume))
	emit_signal("master_volume_changed", _master_volume)

func set_music_volume(volume: float) -> void:
	_music_volume = clamp(volume, 0.0, 1.0)
	AudioServer.set_bus_volume_db(BUS_MUSIC, linear_to_db(_music_volume))
	emit_signal("music_volume_changed", _music_volume)

func set_sfx_volume(volume: float) -> void:
	_sfx_volume = clamp(volume, 0.0, 1.0)
	AudioServer.set_bus_volume_db(BUS_SFX, linear_to_db(_sfx_volume))
	emit_signal("sfx_volume_changed", _sfx_volume)

func _apply_all_volumes() -> void:
	AudioServer.set_bus_volume_db(BUS_MASTER, linear_to_db(_master_volume))
	AudioServer.set_bus_volume_db(BUS_MUSIC, linear_to_db(_music_volume))
	AudioServer.set_bus_volume_db(BUS_SFX, linear_to_db(_sfx_volume))
	AudioServer.set_bus_mute(BUS_MASTER, _master_mute)
	AudioServer.set_bus_mute(BUS_MUSIC, _music_mute)
	AudioServer.set_bus_mute(BUS_SFX, _sfx_mute)

# ===== Getters =====

func is_master_muted() -> bool: return _master_mute
func is_music_muted() -> bool: return _music_mute
func is_sfx_muted() -> bool: return _sfx_mute
func get_master_volume() -> float: return _master_volume
func get_music_volume() -> float: return _music_volume
func get_sfx_volume() -> float: return _sfx_volume