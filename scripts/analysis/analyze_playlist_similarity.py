#!/usr/bin/env python3
"""
Analyze similarity between seed tracks and playlist tracks.
"""

import json
import numpy as np
import psycopg2
from collections import defaultdict

# Seed tracks from the log
SEED_TRACKS = [
    "$uicideboy$ - Paris",
    "$uicideboy$ - Kill Yourself (Part III)",
    "$uicideboy$ - 2nd Hand",
    "$uicideboy$ - ...And to Those I Love, Thanks for Sticking Around",
    "$uicideboy$ - Carrollton",
    "$uicideboy$ - Meet Mr. NICEGUY",
    "$uicideboy$ • GERM - 20th CENTURION",
]

# Playlist tracks (from user's message)
PLAYLIST_TRACKS = """$uicideboy$ - Paris
Noname • Smino • Saba - Ace
Rico Nasty • Don Toliver • Gucci Mane - Don't Like Me (feat. Don Toliver & Gucci Mane)
Zack Fox - fafo
Conway the Machine - Front Lines
Flying Lotus - The Nightcaller
Kid Cudi - WAIT!
2Pac - If I Die 2Nite
They Might Be Giants - Sapphire Bullets of Pure Love
Twista • Miri Ben-Ari - Overnight Celebrity (feat. Miri Ben-Ari)
Lil' Flip - Game Over
Pink Siifu • Fly Anakin - 333GET@ME
$uicideboy$ - Eclipse
Future - Hard To Choose One
42 Dugg • Tae Money - Of Course
Trippie Redd • BIG30 - HIGH HOPES
YG • Nipsey Hussle - FDT
A$AP Twelvyy - Uncle Mikey Skit
Weatherday - Painted Girl's Theme
JAY-Z • Beyoncé - Family Feud
Kay Flock • Dougie B • Chii Wvttz - Don't Trip
Penelope Scott - Rät
Lil Uzi Vert • Oh Wonder - The Way Life Goes (feat. Oh Wonder)
Nas • AZ • Olu Dara - Life's a Bitch (feat. AZ & Olu Dara)
Slipknot - Insert Coin
Arca - Señorita
Janet Jackson - What Have You Done For Me Lately
Knxwledge - watchwhoukallyourhomie
Babyface Ray - 100s
$uicideboy$ - Kill Yourself (Part III)
Real Boston Richey • Lil Crix • Kodak Black - Navy Seals (with Kodak Black & Lil Crix)
Future - Turn On Me
Pooh Shiesty • Foogiano - Take a Life (feat. Foogiano)
Kirk Franklin - Father Knows Best
Comethazine - Piped Up
Future - WORST DAY
Woss Ness - Outta Control Part 2
Anderson .Paak • Jazmine Sullivan - Good Heels (feat. Jazmine Sullivan)
Future - Mad Luv
Playboi Carti - JumpOutTheHouse
Fat Pat - Freestyle (Mack 10 Cavi Hit)
Moneybagg Yo - Still
Sideshow • Madhane - Everything
Yung Lean - Solarflare
Billy Woods • Kenny Segal - Spongebob
42 Dugg - Free Dem Boyz Pt. 2
Isaiah Rashad • Duke Deuce - Lay Wit Ya (feat. Duke Deuce)
OHGEESY - Not a Sound
KIDS SEE GHOSTS • Yasiin Bey - Kids See Ghosts
Shoreline Mafia • OHGEESY • Fenix Flexin - Back 2 Back
Playboi Carti - On That Time
Playboi Carti • Future - Teen X
LUCKI - KYLIE!!!
LUCKI - CTA 2 Bach
Future • Metro Boomin - WTFYM
Charlie Heat • Tdot Illdude - DREAM
Larry June - Things You Do
G Herbo • Southside - Swervo
$uicideboy$ - 2nd Hand
bbno$ • Downtime - too easy
Logic • ADE • Big Lenbo • C Dot Castro • Fat Trel - Gaithersburg Freestyle (feat. C Dot Castro, Big Lenbo, Fat Trel, & ADÉ)
Young Nudy - Blessings
Young Dolph - I'm So Real
Rico Nasty - ON THE LOW
That Mexican OT • DJ Lil Steve - OMG
Jhené Aiko - Mystic Journey
King Von • Booka600 - Jet
Lil B - Miss My Girl
DEA - In The Turning Lane
A$AP Ferg • NAV - What Do You Do (feat. NAV)
The Weeknd - The Knowing
Jay Worthy • Dam Funk • Budgie - Moonlight
DaBaby - XXL
Kodak Black - Needing Something
(G)I-DLE - Senorita
Tee Grizzley - Blueprint
Travis Scott - I Can Tell
Noname • Adam Ness - Prayer Song
Playboi Carti • Red Coldhearted - Middle Of The Summer
Flatbush Zombies • Tech N9ne - Monica (feat. Tech N9ne)
Kali Uchis - //aguardiente y limón %ᵕ‿‿ᵕ%
Daft Punk - Superheroes
Chuuwee • Trizz - Matter Fact
Kali Uchis - Tomorrow
BROCKHAMPTON - WHAT'S THE OCCASION?
Mac DeMarco - All Of Our Yesterdays
Tyler, The Creator - Answer
$uicideboy$ - ...And to Those I Love, Thanks for Sticking Around
Viagra Boys - Shrimp Shack
Griselda • Eminem - Bang
Heavy D • 2Pac • The Notorious B.I.G. • Grand Puba • Spunk Bigga - Let's Get It On
Casey Veggies - New Face$
Blank Banshee - Primordial
Lafayette Afro Rock Band - Little Sister
Big Yavo - New Dude
Matt Champion - Slug
Woesum • Oklou - Empty Lightning (feat. Oklou)
BossMan Dlow - Pressure
Slick Rick • Nas - Me & Nas Bring It To Your Hardest
Zacari • James Fauntleroy - Reverse
Vince Staples - Feels Like Summer
OHGEESY • Kalan.FrFr - Saturday (feat. Kalan.FrFr)
Childish Gambino - III. Telegraph Ave. ("Oakland" by Lloyd)
Famous Dex - THEM DAYS
Migos • YoungBoy Never Broke Again - Need It
Zack Fox - boy i'm on yo ass
Smino • J. Cole - 90 Proof
Woss Ness • Boogieman • Lil' Head • Mista Luv - Get On The Floor
Vince Staples • Mustard - BANG THAT
Yung Lean - Pikachu
Roddy Ricch - Big Stepper
Pouya • Xavier Wulf - whatever mane
JID - Crack Sandwich
Playboi Carti • Kendrick Lamar • Jhené Aiko - BACKD00R
Justice • Miguel - Saturnine
$uicideboy$ - Carrollton
645AR - Yoga
21 Savage - Bank Account
Drake • 21 Savage - Jimmy Cooks
YG - Gimmie Got Shot
EARTHGANG • Kehlani - Trippin
Migos - Get Right Witcha
Baby Keem - gorgeous
Aminé • slowthai • Vince Staples - Pressure In My Palms
EARTHGANG - ALL EYES ON ME
George Benson - In Your Eyes
Lil Peep - move on, be strong
Oneohtrix Point Never - Still Life
Clairo - Softly
GloRilla • Latto - PROCEDURE
Drake - Polar Opposites
LE SSERAFIM - Flash Forward
King Von - No Flaws
Nelly - Hot in Herre
Zelooperz - Bootleg
100 gecs - 757
Ghostemane - Intro.Decadence
Kaliii - Stand On It
Scarlxrd - STEALTH
Chief Keef • Tadoe - Let Me See (feat. Tadoe)
Syd - Nothin to Somethin
Hobo Johnson • Jmsey • Jack Shoot - Ode to Justin Bieber (feat. Jack Shoot & JMSEY)
They Might Be Giants (For Kids) • Marty Beller - Speed and Velocity
$uicideboy$ - Meet Mr. NICEGUY
Chance The Rapper • Nate Fox • Lili K - Pusha Man
Lil Pump • Smokepurpp - Till I See You
Matt Ox - INFINITY SOULS
Young Dolph - What's Da Bizness
Joey Bada$ • CJ FLY - Don't Front
Gunna - Speed It Up
Travis Scott • Kacy Hill - 90210 (feat. Kacy Hill)
Kali Uchis • JHAYCO - la luz(Fín)
Tom Misch - You're On My Mind
Kyle • Kehlani - Playinwitme (feat. Kehlani)
Lil B • The Pack - We Want Some Pussy
21 Savage • Summer Walker - prove it
Almighty Jay • YBN Nahmir • Gucci Mane - New Drip (feat. Gucci Mane)
Eyedress - Mystical Creature's Best Friend
Cootie - Flip
BabyTron - Crocs & Wock'
Travis Scott - Never Catch Me
Billie Eilish - you should see me in a crown
XXXTENTACION - YuNg BrAtZ
XXXTENTACION - #ImSippinTeaInYoHood
Princess Nokia - Excellent
Tyler, The Creator • Steve Lacy • Frank Ocean - 911 / Mr. Lonely (feat. Frank Ocean & Steve Lacy)
That Mexican OT • OG Ron C - Kick Doe Click (ChopNotSlop Remix)
Jungle - Dominoes
Chief Keef - Wazzup
Jay Rock - The Bloodiest
Lil Yachty - Certified
$uicideboy$ • GERM - 20th CENTURION
BROCKHAMPTON - DON'T SHOOT UP THE PARTY
Benny The Butcher • Harry Fraud - Thanksgiving
XXXTENTACION - Hope
Death Grips - Lil Boy
Chance The Rapper - Juice
Big Sean • Tee Grizzley • Kash Doll • Cash Kidd • Payroll • 42 Dugg • Boldy James • Drego • Sada Baby • Royce Da 5'9" • Eminem - Friday Night Cypher
Fat Pat • C-Blount - Friends We Know
Lil Uzi Vert - Malfunction
Lil Uzi Vert - P2
Charli xcx - enemy
Kendrick Lamar - good kid
Young Stoner Life • HiDoraah - Como Te Llama (feat. HiDoraah)
9th Wonder - ToHoldYou!!!
Pusha T - Let The Smokers Shine The Coupes
Tee Grizzley - Dream Youngin
DaBaby • Kevin Gates - POP STAR
Chief Keef • DJ Scream - Blew My High
Olivia Rodrigo - jealousy, jealousy
Yung Lean - God Only Knows
Ian Ewing - Pelican Party
YG • Meek Mill • Arin Ray • Rose Gold - Heart 2 Heart
Lil Baby • Gunna • Lil Durk • NAV - Off White VLONE
Concrete Boys • Lil Yachty • Camo! • KARRAHBOOO • Dc2trill • Draft Day - DIE FOR MINE
Masego • Medasin - Girls That Dance
Giggs • Donaeo - Lock Doh
Joji - COME THRU
JAY-Z • Frank Ocean - Caught Their Eyes""".strip().split('\n')


def cosine_similarity(a, b):
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-10)


def parse_track_name(name):
    """Parse 'Artist - Title' format, handling bullet points for features."""
    # Replace bullet with comma for consistency
    name = name.replace(' • ', ', ')

    if ' - ' in name:
        parts = name.split(' - ', 1)
        return parts[0].strip(), parts[1].strip()
    return name, name


def get_embeddings():
    """Fetch all embeddings from database."""
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

    tracks = {}
    for row in rows:
        track_id, title, artist, album, genres, emb_str = row
        emb = np.array([float(x) for x in emb_str.strip('[]').split(',')])

        # Create lookup key
        key = f"{artist} - {title}".lower()
        tracks[key] = {
            'id': track_id,
            'title': title,
            'artist': artist,
            'album': album,
            'genres': genres,
            'embedding': emb
        }

    return tracks


def find_track(tracks_db, search_name):
    """Find a track in the database by fuzzy matching."""
    search_name = search_name.replace(' • ', ', ').lower()

    # Direct match
    if search_name in tracks_db:
        return tracks_db[search_name]

    # Try partial matches
    for key, track in tracks_db.items():
        # Check if artist and title are contained
        if search_name in key or key in search_name:
            return track

        # Parse and match components
        search_artist, search_title = parse_track_name(search_name)
        if search_artist.lower() in key and search_title.lower() in key:
            return track

    return None


def main():
    print("Fetching embeddings from database...")
    tracks_db = get_embeddings()
    print(f"Loaded {len(tracks_db)} tracks with embeddings\n")

    # Find seed embeddings
    print("=" * 70)
    print("SEED TRACKS")
    print("=" * 70)

    seed_embeddings = []
    seed_names = []
    for seed in SEED_TRACKS:
        track = find_track(tracks_db, seed)
        if track:
            seed_embeddings.append(track['embedding'])
            seed_names.append(seed)
            print(f"  ✓ Found: {seed}")
        else:
            print(f"  ✗ Not found: {seed}")

    if not seed_embeddings:
        print("No seed embeddings found!")
        return

    seed_embeddings = np.array(seed_embeddings)

    # Analyze playlist tracks
    print("\n" + "=" * 70)
    print("PLAYLIST TRACK ANALYSIS")
    print("=" * 70)

    results = []
    not_found = []

    for playlist_track in PLAYLIST_TRACKS:
        track = find_track(tracks_db, playlist_track)
        if track:
            # Compute similarity to each seed
            sims = [cosine_similarity(track['embedding'], seed_emb) for seed_emb in seed_embeddings]
            max_sim = max(sims)
            avg_sim = np.mean(sims)
            closest_seed_idx = np.argmax(sims)

            results.append({
                'name': playlist_track,
                'max_sim': max_sim,
                'avg_sim': avg_sim,
                'closest_seed': seed_names[closest_seed_idx],
                'all_sims': sims,
                'genres': track.get('genres', [])
            })
        else:
            not_found.append(playlist_track)

    # Sort by max similarity (lowest first to find outliers)
    results.sort(key=lambda x: x['max_sim'])

    print(f"\nFound {len(results)} playlist tracks with embeddings")
    print(f"Could not find {len(not_found)} tracks\n")

    # Show worst fitting tracks (outliers)
    print("-" * 70)
    print("WORST FITTING TRACKS (lowest similarity to any seed)")
    print("-" * 70)
    for r in results[:25]:
        print(f"  {r['max_sim']:.4f} (avg: {r['avg_sim']:.4f}) | {r['name'][:55]}")
        print(f"           Closest seed: {r['closest_seed'][:40]}")

    # Show best fitting tracks
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
    print(f"    Median: {np.median(max_sims):.4f}")
    print(f"    Mean:   {np.mean(max_sims):.4f}")
    print(f"    Max:    {max(max_sims):.4f}")

    print(f"\n  Average similarity to all seeds:")
    print(f"    Min:    {min(avg_sims):.4f}")
    print(f"    Median: {np.median(avg_sims):.4f}")
    print(f"    Mean:   {np.mean(avg_sims):.4f}")
    print(f"    Max:    {max(avg_sims):.4f}")

    # Thresholds
    print("\n" + "-" * 70)
    print("TRACKS BY SIMILARITY THRESHOLD")
    print("-" * 70)
    thresholds = [0.85, 0.90, 0.95, 0.98]
    for thresh in thresholds:
        count = sum(1 for r in results if r['max_sim'] < thresh)
        pct = count / len(results) * 100
        print(f"  Below {thresh}: {count} tracks ({pct:.1f}%)")

    # Tracks not found
    if not_found:
        print("\n" + "-" * 70)
        print(f"TRACKS NOT FOUND IN DATABASE ({len(not_found)})")
        print("-" * 70)
        for t in not_found[:20]:
            print(f"  - {t}")
        if len(not_found) > 20:
            print(f"  ... and {len(not_found) - 20} more")

    # Identify problematic outliers (different genre, low similarity)
    print("\n" + "=" * 70)
    print("POTENTIAL OUTLIERS (low sim + different vibe)")
    print("=" * 70)

    # Tracks with sim < 0.92 that are likely not hip-hop/trap
    outlier_candidates = [r for r in results if r['max_sim'] < 0.92]
    print(f"\nTracks with max similarity < 0.92:")
    for r in outlier_candidates:
        print(f"  {r['max_sim']:.4f} | {r['name'][:60]}")


if __name__ == "__main__":
    main()
