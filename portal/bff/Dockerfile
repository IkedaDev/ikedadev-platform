# ==============================================================================
# Stage 1: Build TypeScript BFF (Node 22 Alpine)
# ==============================================================================
FROM node:22-alpine AS builder
WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci

COPY tsconfig.json ./
COPY src ./src
RUN npm run build

# ==============================================================================
# Stage 2: Minimal Production Runtime (Non-Root, Cloud Run Ready)
# ==============================================================================
FROM node:22-alpine AS runtime
WORKDIR /app

ENV NODE_ENV=production \
    PORT=8080

COPY package.json package-lock.json ./
RUN npm ci --omit=dev && npm cache clean --force

COPY --from=builder /app/dist ./dist

USER node

EXPOSE 8080

CMD ["node", "dist/index.js"]
