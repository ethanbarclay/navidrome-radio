#!/usr/bin/env python3
"""
Analyze similarity between seed tracks and station tracks.
Fetches directly from the database for the 'sb' station.
"""

import json
import numpy as np
import psycopg2
from collections import defaultdict

STATION_ID = "46db7e7d-4d03-4b6c-936d-25b78c413852"  # sb station


def cosine_similarity(a, b):
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-10)


def get_connection():
    return psycopg2.connect(
        host="localhost",
        port=5432,
        database="navidrome_radio",
        user="postgres",
        password="postgres"
    )


def get_all_embeddings():
    """Fetch all embeddings from database."""
    conn = get_connection()
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

    tracks = {}
    for row in rows:
        track_id, title, artist, album, genres, emb_str = row
        emb = np.array([float(x) for x in emb_str.strip('[]').split(',')])

        tracks[track_id] = {
            'id': track_id,
            'title': title,
            'artist': artist,
            'album': album,
            'genres': genres,
            'embedding': emb
        }

    return tracks


def get_station_tracks(station_id):
    """Get track IDs for a station."""
    conn = get_connection()
    cur = conn.cursor()

    cur.execute("""
        SELECT track_ids FROM stations WHERE id = %s
    """, (station_id,))

    row = cur.fetchone()
    cur.close()
    conn.close()

    if row and row[0]:
        return row[0]  # Already a list from JSONB
    return []


def identify_seeds(track_ids, all_tracks):
    """Identify which tracks are likely seeds ($uicideboy$ tracks)."""
    seeds = []
    for tid in track_ids:
        if tid in all_tracks:
            track = all_tracks[tid]
            artist = track['artist'].lower()
            if '$uicideboy$' in artist or 'suicideboy' in artist:
                seeds.append(track)
    return seeds


def main():
    print("Fetching embeddings from database...")
    all_tracks = get_all_embeddings()
    print(f"Loaded {len(all_tracks)} tracks with embeddings\n")

    print("Fetching station tracks...")
    station_track_ids = get_station_tracks(STATION_ID)
    print(f"Station has {len(station_track_ids)} tracks\n")

    # Identify seeds (suicideboy$ tracks)
    seeds = identify_seeds(station_track_ids, all_tracks)
    print("=" * 70)
    print(f"SEED TRACKS IDENTIFIED ({len(seeds)} $uicideboy$ tracks)")
    print("=" * 70)
    for s in seeds:
        print(f"  {s['artist']} - {s['title']}")

    if not seeds:
        print("No $uicideboy$ tracks found as seeds!")
        return

    seed_embeddings = np.array([s['embedding'] for s in seeds])

    # Analyze non-seed tracks
    print("\n" + "=" * 70)
    print("PLAYLIST TRACK ANALYSIS")
    print("=" * 70)

    results = []
    missing = 0

    for tid in station_track_ids:
        if tid not in all_tracks:
            missing += 1
            continue

        track = all_tracks[tid]

        # Skip if it's a seed
        if any(s['id'] == tid for s in seeds):
            continue

        # Compute similarity to each seed
        sims = [cosine_similarity(track['embedding'], seed_emb) for seed_emb in seed_embeddings]
        max_sim = max(sims)
        avg_sim = np.mean(sims)
        closest_seed_idx = np.argmax(sims)

        results.append({
            'id': tid,
            'name': f"{track['artist']} - {track['title']}",
            'artist': track['artist'],
            'title': track['title'],
            'max_sim': max_sim,
            'avg_sim': avg_sim,
            'closest_seed': f"{seeds[closest_seed_idx]['artist']} - {seeds[closest_seed_idx]['title']}",
            'all_sims': sims,
            'genres': track.get('genres', [])
        })

    # Sort by max similarity (lowest first)
    results.sort(key=lambda x: x['max_sim'])

    print(f"\nAnalyzed {len(results)} non-seed tracks")
    print(f"Missing embeddings: {missing} tracks\n")

    # Worst fitting tracks
    print("-" * 70)
    print("WORST FITTING TRACKS (lowest similarity to any seed)")
    print("-" * 70)
    for r in results[:30]:
        print(f"  {r['max_sim']:.4f} (avg: {r['avg_sim']:.4f}) | {r['name'][:55]}")

    # Best fitting tracks
    print("\n" + "-" * 70)
    print("BEST FITTING TRACKS (highest similarity to any seed)")
    print("-" * 70)
    for r in results[-15:]:
        print(f"  {r['max_sim']:.4f} (avg: {r['avg_sim']:.4f}) | {r['name'][:55]}")

    # Statistics
    print("\n" + "-" * 70)
    print("SIMILARITY STATISTICS")
    print("-" * 70)
    max_sims = [r['max_sim'] for r in results]
    avg_sims = [r['avg_sim'] for r in results]

    print(f"  Max similarity to any seed:")
    print(f"    Min:    {min(max_sims):.4f}")
    print(f"    25th:   {np.percentile(max_sims, 25):.4f}")
    print(f"    Median: {np.median(max_sims):.4f}")
    print(f"    75th:   {np.percentile(max_sims, 75):.4f}")
    print(f"    Max:    {max(max_sims):.4f}")

    print(f"\n  Average similarity to all seeds:")
    print(f"    Min:    {min(avg_sims):.4f}")
    print(f"    Median: {np.median(avg_sims):.4f}")
    print(f"    Max:    {max(avg_sims):.4f}")

    # Thresholds
    print("\n" + "-" * 70)
    print("TRACKS BY SIMILARITY THRESHOLD")
    print("-" * 70)
    thresholds = [0.90, 0.92, 0.94, 0.96, 0.98]
    for thresh in thresholds:
        count = sum(1 for r in results if r['max_sim'] < thresh)
        pct = count / len(results) * 100
        print(f"  Below {thresh}: {count:3} tracks ({pct:5.1f}%)")

    # Artist distribution in low-similarity tracks
    print("\n" + "-" * 70)
    print("ARTIST BREAKDOWN IN LOW-SIMILARITY TRACKS (sim < 0.94)")
    print("-" * 70)
    low_sim_artists = defaultdict(int)
    for r in results:
        if r['max_sim'] < 0.94:
            low_sim_artists[r['artist']] += 1

    for artist, count in sorted(low_sim_artists.items(), key=lambda x: -x[1])[:20]:
        print(f"  {count:2} tracks: {artist[:50]}")

    # What artists are in high similarity range?
    print("\n" + "-" * 70)
    print("ARTIST BREAKDOWN IN HIGH-SIMILARITY TRACKS (sim >= 0.96)")
    print("-" * 70)
    high_sim_artists = defaultdict(int)
    for r in results:
        if r['max_sim'] >= 0.96:
            high_sim_artists[r['artist']] += 1

    for artist, count in sorted(high_sim_artists.items(), key=lambda x: -x[1])[:20]:
        print(f"  {count:2} tracks: {artist[:50]}")

    # Compute what similarity threshold would be needed to exclude different artists
    print("\n" + "=" * 70)
    print("ANALYSIS: Finding Optimal Similarity Threshold")
    print("=" * 70)

    # Group tracks by whether they're trap/hip-hop sounding
    # Let's look at the spread
    print("\n Similarity distribution:")
    ranges = [(0.85, 0.90), (0.90, 0.92), (0.92, 0.94), (0.94, 0.96), (0.96, 0.98), (0.98, 1.0)]
    for low, high in ranges:
        count = sum(1 for r in results if low <= r['max_sim'] < high)
        print(f"  {low:.2f} - {high:.2f}: {count:3} tracks")

    # Find all $uicideboy$ tracks similarity to seeds
    print("\n" + "=" * 70)
    print("$UICIDEBOY$ TRACKS IN LIBRARY (for comparison)")
    print("=" * 70)

    sb_tracks_library = []
    for tid, track in all_tracks.items():
        if '$uicideboy$' in track['artist'].lower() or 'suicideboy' in track['artist'].lower():
            if not any(s['id'] == tid for s in seeds):
                sims = [cosine_similarity(track['embedding'], seed_emb) for seed_emb in seed_embeddings]
                sb_tracks_library.append({
                    'name': f"{track['artist']} - {track['title']}",
                    'max_sim': max(sims),
                    'avg_sim': np.mean(sims)
                })

    sb_tracks_library.sort(key=lambda x: x['max_sim'])
    print(f"\nOther $uicideboy$ tracks (not seeds): {len(sb_tracks_library)}")
    if sb_tracks_library:
        print(f"  Similarity range: {sb_tracks_library[0]['max_sim']:.4f} - {sb_tracks_library[-1]['max_sim']:.4f}")
        print(f"  These should all be included in an ideal playlist!")


if __name__ == "__main__":
    main()
