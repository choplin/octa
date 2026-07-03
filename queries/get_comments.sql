SELECT
    id         AS "id!: i64",
    body       AS "body!: String",
    created_at AS "created_at!: String"
FROM comments
WHERE issue_number = ?
ORDER BY id;
