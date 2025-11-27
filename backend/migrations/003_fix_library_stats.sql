-- Fix update_library_stats function to handle empty library
CREATE OR REPLACE FUNCTION update_library_stats()
RETURNS void AS $$
BEGIN
    INSERT INTO library_stats (
        total_tracks,
        total_artists,
        total_albums,
        genre_distribution,
        artist_distribution,
        earliest_year,
        latest_year,
        year_distribution,
        mood_distribution,
        avg_energy,
        avg_tempo,
        avg_valence,
        song_type_distribution,
        total_ai_analyzed,
        ai_analysis_percentage
    )
    SELECT
        COUNT(*) as total_tracks,
        COUNT(DISTINCT artist) as total_artists,
        COUNT(DISTINCT album) as total_albums,

        -- Genre distribution (COALESCE to handle empty library)
        COALESCE(
            (SELECT jsonb_object_agg(genre, cnt)
             FROM (
                 SELECT genre, COUNT(*) as cnt
                 FROM library_index, jsonb_array_elements_text(genres) as genre
                 GROUP BY genre
                 ORDER BY cnt DESC
                 LIMIT 50
             ) g),
            '{}'::jsonb
        ) as genre_distribution,

        -- Artist distribution
        COALESCE(
            (SELECT jsonb_object_agg(artist, cnt)
             FROM (
                 SELECT artist, COUNT(*) as cnt
                 FROM library_index
                 GROUP BY artist
                 ORDER BY cnt DESC
                 LIMIT 100
             ) a),
            '{}'::jsonb
        ) as artist_distribution,

        MIN(year) as earliest_year,
        MAX(year) as latest_year,

        -- Year distribution
        COALESCE(
            (SELECT jsonb_object_agg(year::text, cnt)
             FROM (
                 SELECT year, COUNT(*) as cnt
                 FROM library_index
                 WHERE year IS NOT NULL
                 GROUP BY year
                 ORDER BY year
             ) y),
            '{}'::jsonb
        ) as year_distribution,

        -- Mood distribution
        COALESCE(
            (SELECT jsonb_object_agg(mood, cnt)
             FROM (
                 SELECT mood, COUNT(*) as cnt
                 FROM library_index, jsonb_array_elements_text(mood_tags) as mood
                 GROUP BY mood
                 ORDER BY cnt DESC
                 LIMIT 50
             ) m),
            '{}'::jsonb
        ) as mood_distribution,

        AVG(energy_level) as avg_energy,
        AVG(tempo) as avg_tempo,
        AVG(valence) as avg_valence,

        -- Song type distribution
        COALESCE(
            (SELECT jsonb_object_agg(song_type, cnt)
             FROM (
                 SELECT song_type, COUNT(*) as cnt
                 FROM library_index, jsonb_array_elements_text(song_type) as song_type
                 GROUP BY song_type
                 ORDER BY cnt DESC
                 LIMIT 50
             ) st),
            '{}'::jsonb
        ) as song_type_distribution,

        COUNT(*) FILTER (WHERE ai_analyzed = true) as total_ai_analyzed,
        (COUNT(*) FILTER (WHERE ai_analyzed = true)::float / GREATEST(COUNT(*), 1) * 100) as ai_analysis_percentage
    FROM library_index
    ON CONFLICT (id) DO UPDATE SET
        total_tracks = EXCLUDED.total_tracks,
        total_artists = EXCLUDED.total_artists,
        total_albums = EXCLUDED.total_albums,
        genre_distribution = EXCLUDED.genre_distribution,
        artist_distribution = EXCLUDED.artist_distribution,
        earliest_year = EXCLUDED.earliest_year,
        latest_year = EXCLUDED.latest_year,
        year_distribution = EXCLUDED.year_distribution,
        mood_distribution = EXCLUDED.mood_distribution,
        avg_energy = EXCLUDED.avg_energy,
        avg_tempo = EXCLUDED.avg_tempo,
        avg_valence = EXCLUDED.avg_valence,
        song_type_distribution = EXCLUDED.song_type_distribution,
        total_ai_analyzed = EXCLUDED.total_ai_analyzed,
        ai_analysis_percentage = EXCLUDED.ai_analysis_percentage,
        computed_at = NOW();
END;
$$ LANGUAGE plpgsql;
