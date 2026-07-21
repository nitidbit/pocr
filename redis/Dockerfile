FROM redis:8.8-alpine

COPY redis.conf /usr/local/etc/redis/redis.conf
COPY redis-entrypoint.sh /usr/local/bin/redis-entrypoint.sh

RUN chmod +x /usr/local/bin/redis-entrypoint.sh

ENTRYPOINT ["/usr/local/bin/redis-entrypoint.sh"]
CMD ["redis-server", "/usr/local/etc/redis/redis.conf"]
