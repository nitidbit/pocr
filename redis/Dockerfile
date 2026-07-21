FROM redis:8.2-alpine@sha256:223b183cbc49f5ff48728e1fc52ccf101f05072decad2bd9867281a3c9bf75fd

COPY redis.conf /usr/local/etc/redis/redis.conf
COPY redis-entrypoint.sh /usr/local/bin/redis-entrypoint.sh

RUN chmod +x /usr/local/bin/redis-entrypoint.sh

ENTRYPOINT ["/usr/local/bin/redis-entrypoint.sh"]
CMD ["redis-server", "/usr/local/etc/redis/redis.conf"]
