# Navidrome Radio Setup Instructions

## Step 1: Create GitHub Repository

1. Go to https://github.com/new
2. Repository name: `navidrome-radio`
3. Description: "AI-powered radio station generator for Navidrome"
4. Choose **Public** (for free unlimited GitHub Actions) or **Private**
5. **DO NOT** initialize with README, .gitignore, or license
6. Click **Create repository**

## Step 2: Initialize Git and Push Code

```bash
cd /Users/ethanbarclay/Projects/navidrome-radio

# Initialize git if not already done
git init

# Add all files (including lock files which are needed for CI builds)
git add .
git add -f frontend/package-lock.json backend/Cargo.lock

# Commit
git commit -m "Initial commit: Navidrome Radio with unified Docker image"

# Add remote (replace with your actual repo URL)
git remote add origin https://github.com/ethanbarclay/navidrome-radio.git

# Push to GitHub
git branch -M main
git push -u origin main
```

## Step 3: Set Up Docker Hub Secrets

1. Go to your repository: `https://github.com/ethanbarclay/navidrome-radio`
2. Click **Settings** (top right)
3. In left sidebar: **Secrets and variables** → **Actions**
4. Click **New repository secret**
5. Add first secret:
   - Name: `DOCKER_USERNAME`
   - Value: `ethanbarclay` (your Docker Hub username)
   - Click **Add secret**
6. Add second secret:
   - Name: `DOCKER_PASSWORD`
   - Value: Your Docker Hub password or [access token](https://hub.docker.com/settings/security)
   - Click **Add secret**

## Step 4: Enable GitHub Actions

1. Go to **Actions** tab in your repository
2. If prompted, click **I understand my workflows, go ahead and enable them**
3. The workflow will automatically run on the next push

## Step 5: Trigger Your First Build

### Option A: Push a change
```bash
# Make any small change
echo "# Navidrome Radio" > README.md
git add README.md
git commit -m "Add README"
git push
```

### Option B: Manual trigger
1. Go to **Actions** tab
2. Click **Build and Push Docker Images** in the left sidebar
3. Click **Run workflow** button (top right)
4. Select **main** branch
5. Click green **Run workflow** button

## Step 6: Monitor the Build

1. Go to **Actions** tab
2. Click on the running workflow
3. Watch the build progress (takes ~10-15 minutes)
4. When complete, your images will be at:
   - `ethanbarclay/navidrome-radio:latest` (multi-arch: amd64 + arm64)
   - `ethanbarclay/navidrome-radio:sha-<commit>` (specific commit)

## Troubleshooting

### Workflow not running?
- Check **Actions** tab → **Enable workflows** if disabled
- Verify secrets are set correctly

### Build failing?
- Check the logs in **Actions** tab
- Verify Docker Hub credentials in secrets
- Make sure Docker Hub account can create repositories

### Need to rebuild?
- Go to **Actions** tab
- Find the workflow run
- Click **Re-run all jobs**

## Docker Hub Access Token (Recommended)

Instead of using your Docker Hub password, create an access token:
1. Go to https://hub.docker.com/settings/security
2. Click **New Access Token**
3. Description: "GitHub Actions - navidrome-radio"
4. Access permissions: **Read, Write, Delete**
5. Click **Generate**
6. Copy the token
7. Use this as your `DOCKER_PASSWORD` secret
