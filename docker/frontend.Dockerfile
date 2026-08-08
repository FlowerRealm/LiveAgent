# frontend: nginx 服务 WebUI 静态文件 + 反代后端 API。
#
#   docker build -f docker/frontend.Dockerfile -t liveagent-frontend .
#
# 运行时通过环境变量告诉 nginx 后端地址：
#   docker run -e BACKEND_HOST=backend -e BACKEND_PORT=8443 -p 80:80 liveagent-frontend

FROM node:22.19.0-bookworm-slim AS builder

WORKDIR /src/crates/frontend

RUN npm install -g pnpm@10.32.1

COPY crates/frontend/package.json crates/frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

COPY crates/frontend ./
RUN pnpm build

FROM nginx:stable-alpine

COPY --from=builder /src/crates/frontend/dist /usr/share/nginx/html
COPY docker/nginx.conf /etc/nginx/templates/default.conf.template

ENV BACKEND_HOST=backend \
    BACKEND_PORT=8443

EXPOSE 80
