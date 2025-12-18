#!/usr/bin/env python3
"""
Test preprocessing to compare Python librosa with our Rust implementation.
This creates test files that we can compare.
"""

import numpy as np
import librosa
from scipy.ndimage import zoom


def preprocess_audio_python(audio_path: str, sr: int = 22050, n_mels: int = 96,
                            n_fft: int = 2048, hop_length: int = 512,
                            target_frames: int = 216, top_db: float = 80.0):
    """
    Preprocess audio file to mel spectrogram using librosa.
    This is the reference implementation.
    """
    # Load audio
    y, sr = librosa.load(audio_path, sr=sr, mono=True)

    # Compute mel spectrogram (power values)
    mel_spec = librosa.feature.melspectrogram(
        y=y, sr=sr, n_fft=n_fft, hop_length=hop_length, n_mels=n_mels
    )

    # Convert to dB with ref=max, top_db=80
    mel_spec_db = librosa.power_to_db(mel_spec, ref=np.max, top_db=top_db)

    # Normalize to [0, 1]: (S + top_db) / top_db
    mel_spec_norm = (mel_spec_db + top_db) / top_db

    # Resize to target frames
    current_frames = mel_spec_norm.shape[1]
    if current_frames != target_frames:
        scale = target_frames / current_frames
        mel_spec_norm = zoom(mel_spec_norm, (1, scale), order=1)

    # Ensure exact size
    if mel_spec_norm.shape[1] > target_frames:
        mel_spec_norm = mel_spec_norm[:, :target_frames]
    elif mel_spec_norm.shape[1] < target_frames:
        pad = target_frames - mel_spec_norm.shape[1]
        mel_spec_norm = np.pad(mel_spec_norm, ((0, 0), (0, pad)), mode='constant')

    return mel_spec_norm


def create_synthetic_audio_file(output_path: str, duration_secs: float = 6.0,
                                 sample_rate: int = 22050, pattern: str = "sweep"):
    """
    Create a simple synthetic audio file for testing.
    """
    import scipy.io.wavfile as wav

    n_samples = int(duration_secs * sample_rate)
    t = np.linspace(0, duration_secs, n_samples)

    if pattern == "sweep":
        # Frequency sweep from 100 Hz to 8000 Hz
        freq = 100 + 7900 * t / duration_secs
        audio = 0.5 * np.sin(2 * np.pi * freq * t)
    elif pattern == "tone_440":
        # Pure 440 Hz tone
        audio = 0.5 * np.sin(2 * np.pi * 440 * t)
    elif pattern == "tone_220":
        # Pure 220 Hz tone
        audio = 0.5 * np.sin(2 * np.pi * 220 * t)
    elif pattern == "noise":
        # White noise
        audio = 0.3 * np.random.randn(n_samples)
    elif pattern == "silence":
        # Silence with tiny noise
        audio = 0.001 * np.random.randn(n_samples)
    else:
        raise ValueError(f"Unknown pattern: {pattern}")

    # Convert to 16-bit
    audio_int16 = (audio * 32767).astype(np.int16)
    wav.write(output_path, sample_rate, audio_int16)
    print(f"Created {output_path} ({pattern})")


def main():
    import os
    import tempfile

    # Create test audio files
    test_dir = tempfile.mkdtemp()
    patterns = ["sweep", "tone_440", "tone_220", "noise", "silence"]

    audio_files = []
    for pattern in patterns:
        path = os.path.join(test_dir, f"test_{pattern}.wav")
        create_synthetic_audio_file(path, pattern=pattern)
        audio_files.append((pattern, path))

    print("\n" + "="*70)
    print("Python librosa preprocessing results:")
    print("="*70)

    mel_specs = []
    for pattern, path in audio_files:
        mel = preprocess_audio_python(path)
        mel_specs.append((pattern, mel))
        print(f"\n{pattern}:")
        print(f"  Shape: {mel.shape}")
        print(f"  Min: {mel.min():.6f}, Max: {mel.max():.6f}, Mean: {mel.mean():.6f}")
        print(f"  First mel bin stats: min={mel[0,:].min():.4f}, max={mel[0,:].max():.4f}, mean={mel[0,:].mean():.4f}")
        print(f"  Last mel bin stats:  min={mel[-1,:].min():.4f}, max={mel[-1,:].max():.4f}, mean={mel[-1,:].mean():.4f}")

    # Save test files for Rust to read
    print("\n" + "="*70)
    print(f"Test files saved in: {test_dir}")
    print("="*70)

    # Also save the mel spectrograms as numpy files for comparison
    for pattern, mel in mel_specs:
        np_path = os.path.join(test_dir, f"mel_{pattern}.npy")
        np.save(np_path, mel)
        print(f"Saved mel spectrogram: {np_path}")

    # Compute and compare embeddings
    print("\n" + "="*70)
    print("Now loading model and computing embeddings...")
    print("="*70)

    import torch
    from export_audio_encoder import AudioEncoder
    from huggingface_hub import hf_hub_download

    model = AudioEncoder()
    weights_path = hf_hub_download('teticio/audio-encoder', 'diffusion_pytorch_model.bin')
    state_dict = torch.load(weights_path, map_location='cpu', weights_only=True)
    model.load_state_dict(state_dict)
    model.eval()

    embeddings = []
    for pattern, mel in mel_specs:
        # Convert to tensor with batch and channel dims
        tensor = torch.from_numpy(mel.astype(np.float32))
        tensor = tensor.unsqueeze(0).unsqueeze(0)  # (1, 1, 96, 216)

        with torch.no_grad():
            embedding = model(tensor).numpy().flatten()

        embeddings.append((pattern, embedding))
        print(f"\n{pattern}:")
        print(f"  Embedding shape: {embedding.shape}")
        print(f"  Embedding norm: {np.linalg.norm(embedding):.4f}")
        print(f"  Stats: min={embedding.min():.4f}, max={embedding.max():.4f}, mean={embedding.mean():.4f}")

    # Compute pairwise similarities
    print("\n" + "="*70)
    print("Pairwise Cosine Similarities (from librosa mel specs):")
    print("="*70)

    def cosine_similarity(a, b):
        return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-10)

    for i in range(len(embeddings)):
        for j in range(i+1, len(embeddings)):
            name1, emb1 = embeddings[i]
            name2, emb2 = embeddings[j]
            sim = cosine_similarity(emb1, emb2)
            print(f"{name1:15} vs {name2:15}: {sim:.4f}")

    # Cleanup
    import shutil
    print(f"\nCleaning up {test_dir}...")
    shutil.rmtree(test_dir)


if __name__ == "__main__":
    main()
