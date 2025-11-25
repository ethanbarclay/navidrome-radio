# GitHub Actions Workflows

## Docker Build Workflow

This workflow automatically builds and pushes multi-architecture Docker images (AMD64 and ARM64) to Docker Hub.

### Triggers
- **Push to main/master**: Builds and pushes with `latest` tag
- **Pull requests**: Builds only (doesn't push)
- **Tags (v*)**: Builds and pushes with version tags
- **Manual trigger**: Can be run manually from GitHub Actions tab

### Required Secrets
You need to set these secrets in your GitHub repository:
1. `DOCKER_USERNAME` - Your Docker Hub username
2. `DOCKER_PASSWORD` - Your Docker Hub password or access token

### How to Set Up Secrets
1. Go to your repository on GitHub
2. Click **Settings** → **Secrets and variables** → **Actions**
3. Click **New repository secret**
4. Add `DOCKER_USERNAME` with your Docker Hub username
5. Add `DOCKER_PASSWORD` with your Docker Hub password/token

### Manual Trigger
To manually trigger a build:
1. Go to **Actions** tab
2. Click **Build and Push Docker Images**
3. Click **Run workflow**
4. Select branch and click **Run workflow**
