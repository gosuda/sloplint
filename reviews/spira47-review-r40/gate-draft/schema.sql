CREATE DOMAIN name1e AS text;
CREATE DOMAIN name_e AS text;
CREATE DOMAIN name$e AS text;
CREATE DOMAIN namée AS text;
CREATE TABLE items (id bigint PRIMARY KEY, name text NOT NULL, version integer NOT NULL DEFAULT 0);
CREATE DOMAIN version AS text;
CREATE COLLATION version FROM "C";
CREATE FUNCTION public.operator(text) RETURNS text LANGUAGE sql IMMUTABLE AS 'SELECT $1';
