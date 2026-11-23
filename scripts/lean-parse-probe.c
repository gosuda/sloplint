#include <tree_sitter/api.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern const TSLanguage *tree_sitter_lean(void);

typedef struct {
  const char *source;
  uint32_t length;
} Input;

static const char *read_input(void *payload, uint32_t offset, TSPoint point,
                              uint32_t *bytes_read) {
  (void)point;
  const Input *input = payload;
  if (offset >= input->length) {
    *bytes_read = 0;
    return NULL;
  }
  *bytes_read = input->length - offset;
  return input->source + offset;
}

static bool stop_stalled_parse(TSParseState *state) {
  unsigned *checks = state->payload;
  return ++*checks > 10000;
}

static void json_string(const char *value) {
  putchar('"');
  for (const unsigned char *p = (const unsigned char *)value; *p; p++) {
    if (*p == '"' || *p == '\\') {
      putchar('\\');
      putchar(*p);
    } else if (*p < 0x20) {
      printf("\\u%04x", *p);
    } else {
      putchar(*p);
    }
  }
  putchar('"');
}

static char *read_file(const char *path, uint32_t *length) {
  FILE *file = fopen(path, "rb");
  if (!file) {
    perror(path);
    return NULL;
  }
  if (fseek(file, 0, SEEK_END) != 0) {
    perror(path);
    fclose(file);
    return NULL;
  }
  long size = ftell(file);
  if (size < 0 || (unsigned long)size > UINT32_MAX) {
    fprintf(stderr, "unsupported input size: %s\n", path);
    fclose(file);
    return NULL;
  }
  rewind(file);
  char *source = malloc((size_t)size + 1);
  if (!source) {
    perror("malloc");
    fclose(file);
    return NULL;
  }
  if (fread(source, 1, (size_t)size, file) != (size_t)size) {
    fprintf(stderr, "cannot read complete file: %s\n", path);
    free(source);
    fclose(file);
    return NULL;
  }
  fclose(file);
  source[size] = '\0';
  *length = (uint32_t)size;
  return source;
}

static TSNode first_problem(TSNode root) {
  TSTreeCursor cursor = ts_tree_cursor_new(root);
  for (;;) {
    TSNode node = ts_tree_cursor_current_node(&cursor);
    if (ts_node_is_error(node) || ts_node_is_missing(node)) {
      ts_tree_cursor_delete(&cursor);
      return node;
    }
    bool found = false;
    if (ts_tree_cursor_goto_first_child(&cursor)) {
      do {
        if (ts_node_has_error(ts_tree_cursor_current_node(&cursor))) {
          found = true;
          break;
        }
      } while (ts_tree_cursor_goto_next_sibling(&cursor));
    }
    if (!found) {
      ts_tree_cursor_delete(&cursor);
      return node;
    }
  }
}

int main(int argc, char **argv) {
  if (argc < 2) {
    fprintf(stderr, "usage: lean-parse-probe FILE...\n");
    return 2;
  }
  TSParser *parser = ts_parser_new();
  if (!ts_parser_set_language(parser, tree_sitter_lean())) {
    fprintf(stderr, "cannot load Lean language\n");
    ts_parser_delete(parser);
    return 2;
  }
  unsigned clean = 0, errors = 0, cancelled = 0, io_errors = 0;
  for (int i = 1; i < argc; i++) {
    uint32_t length;
    char *source = read_file(argv[i], &length);
    if (!source) {
      io_errors++;
      continue;
    }
    Input payload = {source, length};
    TSInput input = {
      .payload = &payload,
      .read = read_input,
      .encoding = TSInputEncodingUTF8,
    };
    unsigned checks = 0;
    TSParseOptions options = {
      .payload = &checks,
      .progress_callback = stop_stalled_parse,
    };
    ts_parser_reset(parser);
    TSTree *tree = ts_parser_parse_with_options(parser, NULL, input, options);
    printf("{\"path\":");
    json_string(argv[i]);
    printf(",\"bytes\":%u,\"checks\":%u,\"status\":", length, checks);
    if (!tree) {
      json_string("cancelled");
      cancelled++;
    } else if (ts_node_has_error(ts_tree_root_node(tree))) {
      json_string("error");
      TSNode problem = first_problem(ts_tree_root_node(tree));
      TSPoint point = ts_node_start_point(problem);
      printf(",\"row\":%u,\"column\":%u,\"node\":", point.row + 1,
             point.column + 1);
      json_string(ts_node_type(problem));
      errors++;
    } else {
      json_string("clean");
      clean++;
    }
    puts("}");
    ts_tree_delete(tree);
    free(source);
  }
  printf("{\"summary\":{\"files\":%d,\"clean\":%u,\"errors\":%u,"
         "\"cancelled\":%u,\"io_errors\":%u}}\n",
         argc - 1, clean, errors, cancelled, io_errors);
  ts_parser_delete(parser);
  return errors || cancelled || io_errors ? 1 : 0;
}
