CREATE DOMAIN name1e AS text;
CREATE DOMAIN name_e AS text;
CREATE DOMAIN name$e AS text;
CREATE DOMAIN namée AS text;
CREATE TABLE items (id bigint PRIMARY KEY, name text NOT NULL, version integer NOT NULL DEFAULT 0);
