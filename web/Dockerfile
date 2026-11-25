# Build stage. API_PROXY_TARGET is baked into the Next.js rewrites, so it must
# be reachable from inside the web container at build and run time.
FROM node:22-bookworm-slim AS build

WORKDIR /app
ENV NEXT_TELEMETRY_DISABLED=1

COPY package.json package-lock.json ./
RUN npm ci

COPY . .

ARG API_PROXY_TARGET=http://api:8080
ENV API_PROXY_TARGET=${API_PROXY_TARGET}

RUN npm run build

# Runtime stage: Next.js standalone output already contains the minimal
# node_modules it needs.
FROM node:22-bookworm-slim AS runtime

WORKDIR /app
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3000 \
    HOSTNAME=0.0.0.0

COPY --from=build --chown=node:node /app/.next/standalone ./
COPY --from=build --chown=node:node /app/.next/static ./.next/static

USER node
EXPOSE 3000

CMD ["node", "server.js"]
