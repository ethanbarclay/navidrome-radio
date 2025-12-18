#!/usr/bin/env python3
"""
Analyze embedding clusters to verify meaningful audio similarity.
"""

import json
import numpy as np
import psycopg2
from collections import defaultdict
from sklearn.cluster import KMeans
from sklearn.decomposition import PCA
from sklearn.metrics import silhouette_score
from scipy.spatial.distance import cdist


def get_data():
    """Fetch embeddings and metadata from database."""
    conn = psycopg2.connect(
        host="localhost",
        port=5432,
        database="navidrome_radio",
        user="postgres",
        password="postgres"
    )
    cur = conn.cursor()

    # Fetch embeddings with metadata
    cur.execute("""
        SELECT te.track_id, li.title, li.artist, li.album, li.genres,
               te.embedding::text
        FROM track_embeddings te
        JOIN library_index li ON te.track_id = li.id
    """)

    rows = cur.fetchall()
    cur.close()
    conn.close()

    tracks = []
    embeddings = []

    for row in rows:
        track_id, title, artist, album, genres, emb_str = row
        # Parse embedding from postgres vector format "[1,2,3,...]"
        emb = np.array([float(x) for x in emb_str.strip('[]').split(',')])

        tracks.append({
            'id': track_id,
            'title': title,
            'artist': artist,
            'album': album,
            'genres': genres if isinstance(genres, list) else json.loads(genres) if genres else [],
            'primary_genre': (genres[0] if isinstance(genres, list) and genres
                            else json.loads(genres)[0] if genres and json.loads(genres)
                            else 'Unknown')
        })
        embeddings.append(emb)

    return tracks, np.array(embeddings)


def cosine_similarity(a, b):
    """Compute cosine similarity between two vectors."""
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-10)


def analyze_same_artist(tracks, embeddings):
    """Analyze similarity within same artist."""
    print("\n" + "="*70)
    print("SAME ARTIST ANALYSIS")
    print("="*70)

    # Group by artist
    artist_tracks = defaultdict(list)
    for i, track in enumerate(tracks):
        artist_tracks[track['artist']].append(i)

    # Only analyze artists with multiple tracks
    multi_track_artists = {a: indices for a, indices in artist_tracks.items()
                          if len(indices) >= 2}

    print(f"Artists with 2+ tracks: {len(multi_track_artists)}")

    intra_artist_sims = []
    inter_artist_sims = []

    for artist, indices in multi_track_artists.items():
        embs = embeddings[indices]
        # Compute pairwise similarities within artist
        for i in range(len(indices)):
            for j in range(i+1, len(indices)):
                sim = cosine_similarity(embs[i], embs[j])
                intra_artist_sims.append(sim)

    # Sample inter-artist similarities
    all_indices = list(range(len(tracks)))
    np.random.seed(42)
    for _ in range(min(5000, len(intra_artist_sims) * 10)):
        i, j = np.random.choice(all_indices, 2, replace=False)
        if tracks[i]['artist'] != tracks[j]['artist']:
            sim = cosine_similarity(embeddings[i], embeddings[j])
            inter_artist_sims.append(sim)

    print(f"\nIntra-artist similarities ({len(intra_artist_sims)} pairs):")
    print(f"  Mean: {np.mean(intra_artist_sims):.4f}")
    print(f"  Std:  {np.std(intra_artist_sims):.4f}")
    print(f"  Min:  {np.min(intra_artist_sims):.4f}")
    print(f"  Max:  {np.max(intra_artist_sims):.4f}")

    print(f"\nInter-artist similarities ({len(inter_artist_sims)} pairs):")
    print(f"  Mean: {np.mean(inter_artist_sims):.4f}")
    print(f"  Std:  {np.std(inter_artist_sims):.4f}")
    print(f"  Min:  {np.min(inter_artist_sims):.4f}")
    print(f"  Max:  {np.max(inter_artist_sims):.4f}")

    # Show some examples
    print("\n--- Same Artist Examples (sorted by similarity) ---")
    examples = []
    for artist, indices in list(multi_track_artists.items())[:20]:
        if len(indices) >= 2:
            embs = embeddings[indices]
            sim = cosine_similarity(embs[0], embs[1])
            examples.append((artist, tracks[indices[0]]['title'],
                           tracks[indices[1]]['title'], sim))

    examples.sort(key=lambda x: x[3], reverse=True)
    for artist, t1, t2, sim in examples[:10]:
        print(f"  {sim:.4f} | {artist[:30]:30} | {t1[:25]:25} vs {t2[:25]}")

    return np.mean(intra_artist_sims), np.mean(inter_artist_sims)


def analyze_same_genre(tracks, embeddings):
    """Analyze similarity within same genre."""
    print("\n" + "="*70)
    print("SAME GENRE ANALYSIS")
    print("="*70)

    # Group by primary genre
    genre_tracks = defaultdict(list)
    for i, track in enumerate(tracks):
        genre_tracks[track['primary_genre']].append(i)

    print("Genre distribution:")
    for genre in sorted(genre_tracks.keys(), key=lambda g: -len(genre_tracks[g])):
        print(f"  {genre}: {len(genre_tracks[genre])} tracks")

    intra_genre_sims = []
    inter_genre_sims = []

    # Compute intra-genre similarities
    for genre, indices in genre_tracks.items():
        if len(indices) < 2:
            continue
        embs = embeddings[indices]
        # Sample pairs if too many
        if len(indices) > 50:
            sample_indices = np.random.choice(len(indices), 50, replace=False)
            pairs = [(sample_indices[i], sample_indices[j])
                    for i in range(len(sample_indices))
                    for j in range(i+1, len(sample_indices))]
        else:
            pairs = [(i, j) for i in range(len(indices)) for j in range(i+1, len(indices))]

        for i, j in pairs[:500]:  # Cap at 500 pairs per genre
            sim = cosine_similarity(embs[i], embs[j])
            intra_genre_sims.append((genre, sim))

    # Sample inter-genre similarities
    all_indices = list(range(len(tracks)))
    np.random.seed(42)
    for _ in range(5000):
        i, j = np.random.choice(all_indices, 2, replace=False)
        if tracks[i]['primary_genre'] != tracks[j]['primary_genre']:
            sim = cosine_similarity(embeddings[i], embeddings[j])
            inter_genre_sims.append(sim)

    intra_sims = [s for _, s in intra_genre_sims]
    print(f"\nIntra-genre similarities ({len(intra_sims)} pairs):")
    print(f"  Mean: {np.mean(intra_sims):.4f}")
    print(f"  Std:  {np.std(intra_sims):.4f}")
    print(f"  Min:  {np.min(intra_sims):.4f}")
    print(f"  Max:  {np.max(intra_sims):.4f}")

    print(f"\nInter-genre similarities ({len(inter_genre_sims)} pairs):")
    print(f"  Mean: {np.mean(inter_genre_sims):.4f}")
    print(f"  Std:  {np.std(inter_genre_sims):.4f}")
    print(f"  Min:  {np.min(inter_genre_sims):.4f}")
    print(f"  Max:  {np.max(inter_genre_sims):.4f}")

    # Per-genre stats
    print("\n--- Per Genre Mean Similarity ---")
    genre_means = defaultdict(list)
    for genre, sim in intra_genre_sims:
        genre_means[genre].append(sim)

    for genre in sorted(genre_means.keys(), key=lambda g: -np.mean(genre_means[g])):
        sims = genre_means[genre]
        print(f"  {genre:20} mean={np.mean(sims):.4f} std={np.std(sims):.4f} n={len(sims)}")

    return np.mean(intra_sims), np.mean(inter_genre_sims)


def analyze_same_album(tracks, embeddings):
    """Analyze similarity within same album."""
    print("\n" + "="*70)
    print("SAME ALBUM ANALYSIS")
    print("="*70)

    # Group by album
    album_tracks = defaultdict(list)
    for i, track in enumerate(tracks):
        key = (track['artist'], track['album'])
        album_tracks[key].append(i)

    # Only analyze albums with multiple tracks
    multi_track_albums = {a: indices for a, indices in album_tracks.items()
                         if len(indices) >= 2}

    print(f"Albums with 2+ tracks: {len(multi_track_albums)}")

    intra_album_sims = []

    for (artist, album), indices in multi_track_albums.items():
        embs = embeddings[indices]
        for i in range(len(indices)):
            for j in range(i+1, len(indices)):
                sim = cosine_similarity(embs[i], embs[j])
                intra_album_sims.append(sim)

    print(f"\nIntra-album similarities ({len(intra_album_sims)} pairs):")
    print(f"  Mean: {np.mean(intra_album_sims):.4f}")
    print(f"  Std:  {np.std(intra_album_sims):.4f}")
    print(f"  Min:  {np.min(intra_album_sims):.4f}")
    print(f"  Max:  {np.max(intra_album_sims):.4f}")

    # Show some examples
    print("\n--- Same Album Examples ---")
    examples = []
    for (artist, album), indices in list(multi_track_albums.items())[:30]:
        if len(indices) >= 2:
            embs = embeddings[indices]
            sim = cosine_similarity(embs[0], embs[1])
            examples.append((artist, album, tracks[indices[0]]['title'],
                           tracks[indices[1]]['title'], sim))

    examples.sort(key=lambda x: x[4], reverse=True)
    for artist, album, t1, t2, sim in examples[:10]:
        print(f"  {sim:.4f} | {artist[:20]:20} - {album[:20]:20}")
        print(f"         | {t1[:30]} vs {t2[:30]}")

    return np.mean(intra_album_sims)


def kmeans_clustering(tracks, embeddings):
    """Perform K-means clustering and analyze results."""
    print("\n" + "="*70)
    print("K-MEANS CLUSTERING ANALYSIS")
    print("="*70)

    # Try different numbers of clusters
    for n_clusters in [5, 8, 10, 15]:
        kmeans = KMeans(n_clusters=n_clusters, random_state=42, n_init=10)
        labels = kmeans.fit_predict(embeddings)

        # Compute silhouette score
        sil_score = silhouette_score(embeddings, labels, metric='cosine')

        print(f"\nn_clusters={n_clusters}, silhouette={sil_score:.4f}")

        # Analyze cluster composition by genre
        cluster_genres = defaultdict(lambda: defaultdict(int))
        for i, label in enumerate(labels):
            genre = tracks[i]['primary_genre']
            cluster_genres[label][genre] += 1

        print("Cluster composition:")
        for cluster in sorted(cluster_genres.keys()):
            genres = cluster_genres[cluster]
            total = sum(genres.values())
            top_genres = sorted(genres.items(), key=lambda x: -x[1])[:3]
            genre_str = ", ".join(f"{g}:{c}" for g, c in top_genres)
            print(f"  Cluster {cluster} ({total:3} tracks): {genre_str}")


def find_nearest_neighbors(tracks, embeddings):
    """Find and display nearest neighbors for sample tracks."""
    print("\n" + "="*70)
    print("NEAREST NEIGHBOR EXAMPLES")
    print("="*70)

    # Sample diverse tracks
    np.random.seed(42)
    sample_indices = np.random.choice(len(tracks), 10, replace=False)

    for idx in sample_indices:
        track = tracks[idx]
        emb = embeddings[idx]

        # Compute similarities to all other tracks
        sims = []
        for i in range(len(tracks)):
            if i != idx:
                sim = cosine_similarity(emb, embeddings[i])
                sims.append((i, sim))

        sims.sort(key=lambda x: -x[1])

        print(f"\n{track['title'][:40]} by {track['artist'][:30]}")
        print(f"  Genre: {track['primary_genre']}")
        print("  5 Nearest neighbors:")
        for i, sim in sims[:5]:
            neighbor = tracks[i]
            print(f"    {sim:.4f} | {neighbor['title'][:35]:35} | {neighbor['artist'][:25]:25} | {neighbor['primary_genre']}")


def overall_similarity_distribution(embeddings):
    """Analyze overall similarity distribution."""
    print("\n" + "="*70)
    print("OVERALL SIMILARITY DISTRIBUTION")
    print("="*70)

    # Sample pairwise similarities
    n = len(embeddings)
    np.random.seed(42)
    n_samples = min(50000, n * (n - 1) // 2)

    sims = []
    for _ in range(n_samples):
        i, j = np.random.choice(n, 2, replace=False)
        sim = cosine_similarity(embeddings[i], embeddings[j])
        sims.append(sim)

    sims = np.array(sims)

    print(f"Sampled {n_samples} random pairs:")
    print(f"  Mean:   {np.mean(sims):.4f}")
    print(f"  Std:    {np.std(sims):.4f}")
    print(f"  Min:    {np.min(sims):.4f}")
    print(f"  Max:    {np.max(sims):.4f}")
    print(f"  Median: {np.median(sims):.4f}")

    # Distribution buckets
    print("\nSimilarity distribution:")
    buckets = [(0.0, 0.5), (0.5, 0.6), (0.6, 0.7), (0.7, 0.8), (0.8, 0.9), (0.9, 0.95), (0.95, 1.0)]
    for low, high in buckets:
        count = np.sum((sims >= low) & (sims < high))
        pct = 100 * count / len(sims)
        print(f"  [{low:.2f}, {high:.2f}): {count:5} ({pct:5.1f}%)")


def main():
    print("Fetching embeddings from database...")
    tracks, embeddings = get_data()
    print(f"Loaded {len(tracks)} tracks with {embeddings.shape[1]}-dim embeddings")

    # Normalize embeddings for cosine similarity
    norms = np.linalg.norm(embeddings, axis=1, keepdims=True)
    embeddings_normed = embeddings / (norms + 1e-10)

    # Overall statistics
    overall_similarity_distribution(embeddings)

    # Same artist analysis
    intra_artist, inter_artist = analyze_same_artist(tracks, embeddings)

    # Same genre analysis
    intra_genre, inter_genre = analyze_same_genre(tracks, embeddings)

    # Same album analysis
    intra_album = analyze_same_album(tracks, embeddings)

    # K-means clustering
    kmeans_clustering(tracks, embeddings)

    # Nearest neighbors
    find_nearest_neighbors(tracks, embeddings)

    # Summary
    print("\n" + "="*70)
    print("SUMMARY")
    print("="*70)
    print(f"Same artist tracks are more similar:  {intra_artist:.4f} vs {inter_artist:.4f} (diff: {intra_artist - inter_artist:+.4f})")
    print(f"Same genre tracks are more similar:   {intra_genre:.4f} vs {inter_genre:.4f} (diff: {intra_genre - inter_genre:+.4f})")
    print(f"Same album mean similarity:           {intra_album:.4f}")

    if intra_artist > inter_artist and intra_genre > inter_genre:
        print("\n✓ Embeddings show meaningful structure!")
        print("  - Tracks from same artist cluster together")
        print("  - Tracks from same genre cluster together")
    else:
        print("\n✗ Embeddings may not be capturing meaningful audio features")


if __name__ == "__main__":
    main()
