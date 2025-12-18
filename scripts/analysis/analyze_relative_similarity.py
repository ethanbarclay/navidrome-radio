#!/usr/bin/env python3
"""
Analyze relative similarity in embeddings.
Focus on: Can the model distinguish similar vs different music?
"""

import json
import numpy as np
import psycopg2
from collections import defaultdict


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
        emb = np.array([float(x) for x in emb_str.strip('[]').split(',')])

        genres_list = genres if isinstance(genres, list) else json.loads(genres) if genres else []

        tracks.append({
            'id': track_id,
            'title': title,
            'artist': artist,
            'album': album,
            'genres': genres_list,
            'primary_genre': genres_list[0] if genres_list else 'Unknown'
        })
        embeddings.append(emb)

    return tracks, np.array(embeddings)


def cosine_similarity(a, b):
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-10)


def categorize_genre(genre):
    """Map specific genres to broader categories."""
    genre_lower = genre.lower()

    # Heavy/aggressive
    if any(x in genre_lower for x in ['metal', 'punk', 'hardcore']):
        return 'Heavy'

    # Electronic
    if any(x in genre_lower for x in ['electro', 'electronic', 'dance', 'edm', 'house', 'techno']):
        return 'Electronic'

    # Hip-hop/Rap
    if any(x in genre_lower for x in ['rap', 'hip hop', 'hip-hop', 'screwed']):
        return 'Hip-Hop'

    # R&B/Soul
    if any(x in genre_lower for x in ['r&b', 'soul', 'funk']):
        return 'R&B/Soul'

    # Rock
    if any(x in genre_lower for x in ['rock', 'indie']):
        return 'Rock'

    # Jazz
    if 'jazz' in genre_lower:
        return 'Jazz'

    # Pop
    if 'pop' in genre_lower:
        return 'Pop'

    # Country
    if 'country' in genre_lower:
        return 'Country'

    # Alternative
    if 'alternative' in genre_lower:
        return 'Alternative'

    return 'Other'


def analyze_cross_genre(tracks, embeddings):
    """Analyze similarity between very different genres."""
    print("\n" + "="*70)
    print("CROSS-GENRE SIMILARITY ANALYSIS")
    print("="*70)

    # Categorize tracks
    for track in tracks:
        track['category'] = categorize_genre(track['primary_genre'])

    # Count categories
    category_counts = defaultdict(int)
    for track in tracks:
        category_counts[track['category']] += 1

    print("\nCategory distribution:")
    for cat in sorted(category_counts.keys(), key=lambda x: -category_counts[x]):
        print(f"  {cat}: {category_counts[cat]} tracks")

    # Group by category
    category_indices = defaultdict(list)
    for i, track in enumerate(tracks):
        category_indices[track['category']].append(i)

    # Compute within-category and between-category similarities
    categories = [c for c in category_indices.keys() if len(category_indices[c]) >= 5]

    print(f"\nAnalyzing {len(categories)} categories with 5+ tracks")

    # Within-category similarities
    within_sims = {}
    for cat in categories:
        indices = category_indices[cat]
        sims = []
        for i in range(len(indices)):
            for j in range(i+1, len(indices)):
                sim = cosine_similarity(embeddings[indices[i]], embeddings[indices[j]])
                sims.append(sim)
        if sims:
            within_sims[cat] = np.mean(sims)

    # Between-category similarities
    between_sims = {}
    for i, cat1 in enumerate(categories):
        for cat2 in categories[i+1:]:
            indices1 = category_indices[cat1]
            indices2 = category_indices[cat2]
            sims = []
            # Sample pairs
            np.random.seed(42)
            n_samples = min(200, len(indices1) * len(indices2))
            for _ in range(n_samples):
                idx1 = np.random.choice(indices1)
                idx2 = np.random.choice(indices2)
                sim = cosine_similarity(embeddings[idx1], embeddings[idx2])
                sims.append(sim)
            between_sims[(cat1, cat2)] = np.mean(sims)

    # Print within-category
    print("\n--- Within-Category Mean Similarity ---")
    for cat in sorted(within_sims.keys(), key=lambda x: -within_sims[x]):
        print(f"  {cat:15}: {within_sims[cat]:.4f}")

    # Print between-category matrix
    print("\n--- Between-Category Mean Similarity ---")
    print(f"{'':15}", end="")
    for cat in categories:
        print(f"{cat[:8]:>9}", end="")
    print()

    for cat1 in categories:
        print(f"{cat1:15}", end="")
        for cat2 in categories:
            if cat1 == cat2:
                print(f"{'--':>9}", end="")
            else:
                key = (cat1, cat2) if (cat1, cat2) in between_sims else (cat2, cat1)
                print(f"{between_sims.get(key, 0):.4f}   ", end="")
        print()

    # Find most and least similar category pairs
    print("\n--- Category Pair Rankings (most to least similar) ---")
    sorted_pairs = sorted(between_sims.items(), key=lambda x: -x[1])
    for (cat1, cat2), sim in sorted_pairs:
        print(f"  {sim:.4f}: {cat1} <-> {cat2}")

    return within_sims, between_sims


def analyze_extreme_comparisons(tracks, embeddings):
    """Find and compare most similar vs most different tracks."""
    print("\n" + "="*70)
    print("EXTREME COMPARISONS")
    print("="*70)

    # Compute all pairwise similarities (for smaller datasets)
    n = len(tracks)
    print(f"\nComputing pairwise similarities for {n} tracks...")

    all_pairs = []
    for i in range(n):
        for j in range(i+1, n):
            sim = cosine_similarity(embeddings[i], embeddings[j])
            all_pairs.append((i, j, sim))

    all_pairs.sort(key=lambda x: x[2])

    # Most different pairs
    print("\n--- 15 MOST DIFFERENT Track Pairs ---")
    for i, j, sim in all_pairs[:15]:
        t1, t2 = tracks[i], tracks[j]
        print(f"  {sim:.4f} | {t1['title'][:25]:25} ({t1['primary_genre'][:12]:12}) vs {t2['title'][:25]:25} ({t2['primary_genre'][:12]})")

    # Most similar pairs (excluding same artist)
    print("\n--- 15 MOST SIMILAR Track Pairs (different artists) ---")
    same_artist_excluded = [(i, j, sim) for i, j, sim in all_pairs
                            if tracks[i]['artist'] != tracks[j]['artist']]
    for i, j, sim in same_artist_excluded[-15:]:
        t1, t2 = tracks[i], tracks[j]
        print(f"  {sim:.4f} | {t1['title'][:25]:25} ({t1['artist'][:15]:15}) vs {t2['title'][:25]:25} ({t2['artist'][:15]})")

    # Similarity range
    min_sim = all_pairs[0][2]
    max_sim = all_pairs[-1][2]
    median_sim = all_pairs[len(all_pairs)//2][2]

    print(f"\n--- Similarity Range ---")
    print(f"  Min: {min_sim:.4f}")
    print(f"  Median: {median_sim:.4f}")
    print(f"  Max: {max_sim:.4f}")
    print(f"  Range: {max_sim - min_sim:.4f}")

    return all_pairs


def analyze_genre_separation(tracks, embeddings):
    """Check if specific genres are separable."""
    print("\n" + "="*70)
    print("GENRE SEPARATION ANALYSIS")
    print("="*70)

    # Find pairs of "opposite" genres
    test_pairs = [
        ('Rap', 'Jazz'),
        ('Rap', 'Rock'),
        ('Rap', 'Country'),
        ('Electronic', 'Jazz'),
        ('Electronic', 'Country'),
        ('Metal', 'Jazz'),
        ('Hip-Hop', 'Country'),
        ('R&B', 'Metal'),
    ]

    # Categorize
    for track in tracks:
        track['category'] = categorize_genre(track['primary_genre'])

    category_indices = defaultdict(list)
    for i, track in enumerate(tracks):
        category_indices[track['category']].append(i)

    print("\n--- Same vs Different Genre Comparison ---")
    print(f"{'Genre Pair':30} | {'Same-Genre':12} | {'Cross-Genre':12} | {'Separation':12}")
    print("-" * 70)

    for cat1, cat2 in test_pairs:
        if cat1 not in category_indices or cat2 not in category_indices:
            continue
        if len(category_indices[cat1]) < 3 or len(category_indices[cat2]) < 3:
            continue

        # Within cat1
        indices1 = category_indices[cat1]
        within_sims = []
        for i in range(len(indices1)):
            for j in range(i+1, len(indices1)):
                sim = cosine_similarity(embeddings[indices1[i]], embeddings[indices1[j]])
                within_sims.append(sim)

        # Between cat1 and cat2
        indices2 = category_indices[cat2]
        between_sims = []
        np.random.seed(42)
        for _ in range(min(200, len(indices1) * len(indices2))):
            idx1 = np.random.choice(indices1)
            idx2 = np.random.choice(indices2)
            sim = cosine_similarity(embeddings[idx1], embeddings[idx2])
            between_sims.append(sim)

        within_mean = np.mean(within_sims) if within_sims else 0
        between_mean = np.mean(between_sims) if between_sims else 0
        separation = within_mean - between_mean

        print(f"{cat1:12} vs {cat2:12} | {within_mean:.4f}       | {between_mean:.4f}       | {separation:+.4f}")


def find_outliers_per_genre(tracks, embeddings):
    """Find tracks that don't fit their genre."""
    print("\n" + "="*70)
    print("GENRE OUTLIERS (tracks that don't match their genre embedding-wise)")
    print("="*70)

    # Categorize
    for track in tracks:
        track['category'] = categorize_genre(track['primary_genre'])

    category_indices = defaultdict(list)
    for i, track in enumerate(tracks):
        category_indices[track['category']].append(i)

    for cat in ['Hip-Hop', 'Rock', 'Electronic', 'Jazz', 'Alternative']:
        if cat not in category_indices or len(category_indices[cat]) < 5:
            continue

        indices = category_indices[cat]

        # Compute mean similarity to own genre for each track
        track_genre_fit = []
        for idx in indices:
            sims = []
            for other_idx in indices:
                if idx != other_idx:
                    sim = cosine_similarity(embeddings[idx], embeddings[other_idx])
                    sims.append(sim)
            mean_sim = np.mean(sims) if sims else 0
            track_genre_fit.append((idx, mean_sim))

        track_genre_fit.sort(key=lambda x: x[1])

        print(f"\n{cat} outliers (lowest fit to genre):")
        for idx, fit in track_genre_fit[:3]:
            t = tracks[idx]
            print(f"  {fit:.4f} | {t['title'][:30]:30} by {t['artist'][:20]}")


def nearest_neighbor_accuracy(tracks, embeddings):
    """Check if nearest neighbors tend to be same genre."""
    print("\n" + "="*70)
    print("NEAREST NEIGHBOR GENRE ACCURACY")
    print("="*70)

    # Categorize
    for track in tracks:
        track['category'] = categorize_genre(track['primary_genre'])

    # For each track, find k nearest neighbors and check genre match
    k_values = [1, 3, 5, 10]

    for k in k_values:
        correct = 0
        total = 0

        for i in range(len(tracks)):
            # Compute similarity to all other tracks
            sims = []
            for j in range(len(tracks)):
                if i != j:
                    sim = cosine_similarity(embeddings[i], embeddings[j])
                    sims.append((j, sim))

            sims.sort(key=lambda x: -x[1])
            neighbors = sims[:k]

            # Check how many neighbors share the same category
            same_cat = sum(1 for j, _ in neighbors if tracks[j]['category'] == tracks[i]['category'])
            correct += same_cat
            total += k

        accuracy = correct / total if total > 0 else 0
        print(f"  k={k:2}: {accuracy:.2%} of nearest neighbors share genre category")


def main():
    print("Fetching embeddings from database...")
    tracks, embeddings = get_data()
    print(f"Loaded {len(tracks)} tracks with {embeddings.shape[1]}-dim embeddings")

    # Cross-genre analysis
    analyze_cross_genre(tracks, embeddings)

    # Extreme comparisons
    analyze_extreme_comparisons(tracks, embeddings)

    # Genre separation
    analyze_genre_separation(tracks, embeddings)

    # Genre outliers
    find_outliers_per_genre(tracks, embeddings)

    # Nearest neighbor accuracy
    nearest_neighbor_accuracy(tracks, embeddings)

    print("\n" + "="*70)
    print("CONCLUSION")
    print("="*70)
    print("""
The key question: Can embeddings distinguish similar vs different music?

Look at:
1. Cross-genre similarity - different genres should have LOWER similarity
2. Separation scores - positive = same-genre more similar than cross-genre
3. Nearest neighbor accuracy - higher % = embeddings respect genre boundaries
4. Extreme pairs - most different tracks should be from different genres
""")


if __name__ == "__main__":
    main()
