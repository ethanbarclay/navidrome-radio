#!/usr/bin/env python3
"""
Test the audio encoder with synthetic mel spectrograms.
This tests the model directly without needing audio files.
"""

import numpy as np
import torch

# Load the same model we exported
from export_audio_encoder import AudioEncoder
from huggingface_hub import hf_hub_download


def load_model():
    """Load the audio encoder model with trained weights."""
    model = AudioEncoder()
    weights_path = hf_hub_download('teticio/audio-encoder', 'diffusion_pytorch_model.bin')
    state_dict = torch.load(weights_path, map_location='cpu', weights_only=True)
    model.load_state_dict(state_dict)
    model.eval()
    return model


def create_synthetic_mel(pattern: str, n_mels: int = 96, target_frames: int = 216) -> torch.Tensor:
    """
    Create synthetic mel spectrograms that simulate different audio characteristics.
    Values are in [0, 1] range (normalized like the preprocessing does).
    """
    mel = np.zeros((n_mels, target_frames), dtype=np.float32)

    if pattern == "bass_heavy":
        # Strong energy in low frequencies
        mel[:30, :] = np.random.uniform(0.6, 1.0, (30, target_frames))
        mel[30:, :] = np.random.uniform(0.0, 0.3, (n_mels-30, target_frames))

    elif pattern == "treble_heavy":
        # Strong energy in high frequencies
        mel[:40, :] = np.random.uniform(0.0, 0.3, (40, target_frames))
        mel[40:, :] = np.random.uniform(0.6, 1.0, (n_mels-40, target_frames))

    elif pattern == "mid_range":
        # Strong energy in mid frequencies
        mel[:30, :] = np.random.uniform(0.1, 0.3, (30, target_frames))
        mel[30:60, :] = np.random.uniform(0.6, 1.0, (30, target_frames))
        mel[60:, :] = np.random.uniform(0.1, 0.3, (n_mels-60, target_frames))

    elif pattern == "rhythmic":
        # Alternating energy (simulates beats)
        for i in range(0, target_frames, 10):
            mel[:, i:i+3] = np.random.uniform(0.7, 1.0, (n_mels, min(3, target_frames-i)))
            if i+3 < target_frames:
                mel[:, i+3:min(i+10, target_frames)] = np.random.uniform(0.1, 0.3, (n_mels, min(7, target_frames-i-3)))

    elif pattern == "quiet":
        # Mostly quiet with occasional activity
        mel[:, :] = np.random.uniform(0.0, 0.2, (n_mels, target_frames))
        # Add some random bursts
        for _ in range(5):
            start = np.random.randint(0, target_frames - 20)
            mel[:, start:start+20] = np.random.uniform(0.4, 0.7, (n_mels, 20))

    elif pattern == "full_spectrum":
        # Full spectrum noise (white noise-like)
        mel[:, :] = np.random.uniform(0.5, 0.9, (n_mels, target_frames))

    elif pattern == "silence":
        # Nearly silent
        mel[:, :] = np.random.uniform(0.0, 0.1, (n_mels, target_frames))

    else:  # random
        mel[:, :] = np.random.uniform(0.0, 1.0, (n_mels, target_frames))

    # Convert to tensor with batch and channel dimensions
    tensor = torch.from_numpy(mel)
    tensor = tensor.unsqueeze(0).unsqueeze(0)  # (1, 1, n_mels, target_frames)

    return tensor


def cosine_similarity(a, b):
    """Compute cosine similarity between two vectors."""
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-10)


def main():
    print("Loading model...")
    model = load_model()

    # Test patterns
    patterns = [
        "bass_heavy",
        "treble_heavy",
        "mid_range",
        "rhythmic",
        "quiet",
        "full_spectrum",
        "silence",
        "random",
    ]

    print("\nGenerating embeddings for synthetic patterns...")
    embeddings = []

    for pattern in patterns:
        mel_tensor = create_synthetic_mel(pattern)
        print(f"\n{pattern}:")
        print(f"  Mel shape: {mel_tensor.shape}")
        print(f"  Mel stats: min={mel_tensor.min():.4f}, max={mel_tensor.max():.4f}, mean={mel_tensor.mean():.4f}")

        with torch.no_grad():
            embedding = model(mel_tensor).numpy().flatten()

        print(f"  Embedding stats: min={embedding.min():.4f}, max={embedding.max():.4f}, mean={embedding.mean():.4f}")
        print(f"  Embedding norm: {np.linalg.norm(embedding):.4f}")
        print(f"  First 5 values: {embedding[:5]}")

        embeddings.append((pattern, embedding))

    # Compute pairwise similarities
    print("\n" + "="*70)
    print("Pairwise Cosine Similarities:")
    print("="*70)

    similarities = []
    for i in range(len(embeddings)):
        for j in range(i+1, len(embeddings)):
            name1, emb1 = embeddings[i]
            name2, emb2 = embeddings[j]
            sim = cosine_similarity(emb1, emb2)
            similarities.append(sim)
            print(f"{name1:20} vs {name2:20}: {sim:.4f}")

    print("\n" + "="*70)
    print("Summary Statistics:")
    print("="*70)
    print(f"Min similarity:  {min(similarities):.4f}")
    print(f"Max similarity:  {max(similarities):.4f}")
    print(f"Mean similarity: {np.mean(similarities):.4f}")
    print(f"Std similarity:  {np.std(similarities):.4f}")

    # Check if model differentiates
    if max(similarities) - min(similarities) > 0.3:
        print("\n✓ Model shows good differentiation between patterns!")
    elif max(similarities) - min(similarities) > 0.1:
        print("\n⚠ Model shows some differentiation, but range is narrow")
    else:
        print("\n✗ Model shows very little differentiation - all embeddings similar!")


if __name__ == "__main__":
    main()
