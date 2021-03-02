#include <stdio.h>
#include <stdlib.h>

typedef enum type {INT, STRING, PAIR, LIST, DICT} type;

// byte string, not "string"
// no null terminator
typedef struct {
    char *str;
    int len;
} String;

typedef struct Node {
    void *data;
    struct Node *next;
    type t;
} Node;

typedef struct {
    Node *head;
    Node *tail;
} List;

void append(List *list, void *data, type t) {
    Node *node = calloc(1, sizeof(Node));
    node->data = data;
    node->next = NULL;
    node->t = t;
    if(list->head == NULL) {
        list->head = node;
        list->tail = node;
    } else {
        list->tail->next = node;
        list->tail = node;
    }
}

typedef struct {
    String *key;
    void *val;
    type t;
} Pair;

static int *parse_int(char *str) {
    int *num = calloc(1, sizeof(int));
    sscanf(str, "i%de", num);
    return num;
}

static int get_int_len(char *str) {
    char *start = str;
    for(; *str != 'e'; str++) {} str++;
    return str-start;
}

static String *parse_str(char *str) {
    int len;
    sscanf(str, "%d:", &len);
    char *ret = calloc(len, 1);
    // advance past len:
    for(; *str != ':'; str++) {} str++;
    // read len bytes
    for(int i = 0; i < len; i++) ret[i] = str[i];
    // store byte string
    String *s = calloc(1, sizeof(String));
    s->str = ret;
    s->len = len;
    return s;
}

static int get_str_len(char *str) {
    int len;
    sscanf(str, "%d:", &len);
    int i = 0;
    for(; str[i] != ':'; i++) {}
    return len+i+1;
}

static int get_dict_len(char *str);

// uses parser advancing logic
static int get_list_len(char *str) {
    char *start = str; str++;
    while(*str != 'e') {
        if(*str == 'i') str += get_int_len(str);
        else if(*str == 'l') str += get_list_len(str);
        else if(*str == 'd') str += get_dict_len(str);
        else str += get_str_len(str);
    }
    return (str-start)+1;
}

static List *parse_dict(char *str);

static List *parse_list(char *str) {
    List *list = calloc(1, sizeof(List)); str++;
    while(*str != 'e') {
        if(*str == 'i') {
            append(list, parse_int(str), INT);
            // advance past int
            str += get_int_len(str);
        } else if(*str == 'l') {
            append(list, parse_list(str), LIST);
            // advance past list
            str += get_list_len(str);
        } else if(*str == 'd') {
            append(list, parse_dict(str), DICT);
            // advance past dict
            str += get_dict_len(str);
        } else {
            append(list, parse_str(str), STRING);
            // advance past str
            str += get_str_len(str);
        }
    }
    return list;
}

static Pair *parse_pair(char *str) {
    Pair *pair = calloc(1, sizeof(Pair));
    pair->key = parse_str(str);
    // advance past str
    str += get_str_len(str);
    if(*str == 'i') {
        pair->val = parse_int(str);
        pair->t = INT;
    } else if(*str == 'l') {
        pair->val = parse_list(str);
        pair->t = LIST;
    } else if(*str == 'd') {
        pair->val = parse_dict(str);
        pair->t = DICT;
    } else {
        pair->val = parse_str(str);
        pair->t = STRING;
    }
    return pair;
}

static int get_pair_len(char *str) {
    int len = get_str_len(str);
    str += len;
    if(*str == 'i') len += get_int_len(str);
    else if(*str == 'l') len += get_list_len(str);
    else if(*str == 'd') len += get_dict_len(str);
    else len += get_str_len(str);
    return len;
}

static List *parse_dict(char *str) {
    List *dict = calloc(1, sizeof(List)); str++;
    while(*str != 'e') {
        append(dict, parse_pair(str), PAIR);
        str += get_pair_len(str);
    }
    return dict;
}

static int get_dict_len(char *str) {
    char *start = str; str++;
    while(*str != 'e') str += get_pair_len(str);
    return (str-start)+1;
}

List *parse(char *str) {
    List *tree = calloc(1, sizeof(List));
    while(*str != 0 && *str != 'e') {
        if(*str == 'i') {
            append(tree, parse_int(str), INT);
            // advance past int
            str += get_int_len(str);
        } else if(*str == 'l') {
            append(tree, parse_list(str), LIST);
            // advance past list
            str += get_list_len(str);
        } else if(*str == 'd') {
            append(tree, parse_dict(str), DICT);
            // advance past dict
            str += get_dict_len(str);
        } else {
            append(tree, parse_str(str), STRING);
            // advance past str
            str += get_str_len(str);
        }
    }
    return tree;
}

static void print_list(List *list);

static void print_str(String *str) {
    for(int i = 0; i < str->len; i++) printf("%c", str->str[i]); 
}

static void print_dict(List *dict) {
    printf("{");
    for(Node *cur = dict->head; cur != NULL; cur = cur->next) {
        Pair *pair = cur->data;
        String *key = pair->key;
        if(pair->t == INT) {
            int *num = pair->val;
            print_str(key);
            printf(":%d, ", *num);
        } else if(pair->t == LIST) {
            print_str(key);
            printf(":");
            print_list(pair->val);
            printf(", ");
        } else if(pair->t == DICT) {
            print_str(key);
            printf(":");
            print_dict(pair->val);
            printf(", ");
        } else if(pair->t == STRING) {
            String *val = pair->val;
            print_str(key);
            printf(":");
            print_str(val);
            printf(", ");
        }
    }
    printf("} ");
}

static void print_list(List *list) {
    printf("[");
    for(Node *cur = list->head; cur != NULL; cur = cur->next) {
        if(cur->t == INT) {
            int *num = cur->data;
            printf("%d, ", *num);
        } else if(cur->t == LIST) {
            print_list(cur->data);
            printf(", ");
        } else if(cur->t == DICT) {
            print_dict(cur->data);
            printf(", ");
        } else if(cur->t == STRING) {
            print_str(cur->data);
            printf(", ");
        }
    }
    printf("] ");
}

void print_tree(List *tree) {
    for(Node *cur = tree->head; cur != NULL; cur = cur->next) {
        if(cur->t == INT) {
            int *num = cur->data;
            printf("%d\n", *num);
        } else if(cur->t == LIST) {
            print_list(cur->data);
            printf("\n");
        } else if(cur->t == DICT) {
            print_dict(cur->data);
            printf("\n");
        } else if(cur->t == STRING) {
            print_str(cur->data);
            printf("\n");
        }
    }
}

void free_ll(List *tree);

void free_str(String *str) {
    free(str->str);
    free(str);
}

void free_pair(Pair *pair) {
    String *key = pair->key;
    free(key->str);
    free(key);
    if(pair->t == LIST || pair->t == DICT) {
        free_ll(pair->val);
    } else if(pair->t == PAIR) {
        free_pair(pair->val);
    } else if(pair->t == STRING) {
        free_str(pair->val);
    } else {
        free(pair->val);
    }
    free(pair);
}

void free_ll(List *tree) {
    Node *f = tree->head;
    Node *g;
    while(f != NULL) {
        g = f;
        f = f->next;
        if(g->t == LIST || g->t == DICT) {
            free_ll(g->data);
        } else if(g->t == PAIR) {
            free_pair(g->data);
        } else if(g->t == STRING) {
            free_str(g->data);
        } else { 
            free(g->data);
        }
        free(g);
    }
    free(tree);
}