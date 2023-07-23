// @generated automatically by Diesel CLI.

diesel::table! {
    jobs (id) {
        id -> Nullable<Integer>,
        task_id -> Integer,
        inputs -> Text,
        output -> Text,
        created_at -> Timestamp,
        started_at -> Nullable<Timestamp>,
        finished_at -> Nullable<Timestamp>,
        success -> Nullable<Bool>,
    }
}

diesel::table! {
    peers (uuid) {
        uuid -> Nullable<Binary>,
        created_at -> Integer,
        ips -> Binary,
    }
}

diesel::table! {
    tasks (id) {
        id -> Nullable<Integer>,
        name -> Text,
        args -> Text,
        created_at -> Timestamp,
        started_at -> Nullable<Timestamp>,
        finished_at -> Nullable<Timestamp>,
    }
}

diesel::joinable!(jobs -> tasks (task_id));

diesel::allow_tables_to_appear_in_same_query!(
    jobs,
    peers,
    tasks,
);
