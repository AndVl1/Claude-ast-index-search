import { useState, useEffect, useMemo, useCallback } from "react";
import type { Dispatch, SetStateAction } from "react";

export interface User {
    id: string;
    name: string;
    email: string;
    createdAt: number;
}

export interface Post {
    id: string;
    title: string;
    body: string;
    authorId: string;
    tags: string[];
}

export type Loadable<T> =
    | { status: "idle" }
    | { status: "loading" }
    | { status: "success"; data: T }
    | { status: "error"; message: string };

export class ApiClient {
    constructor(private baseUrl: string, private token?: string) {}

    private headers(): Record<string, string> {
        const h: Record<string, string> = { "Content-Type": "application/json" };
        if (this.token) {
            h["Authorization"] = `Bearer ${this.token}`;
        }
        return h;
    }

    async getUser(id: string): Promise<User> {
        const res = await fetch(`${this.baseUrl}/users/${id}`, {
            headers: this.headers(),
        });
        if (!res.ok) {
            throw new Error(`HTTP ${res.status}`);
        }
        return (await res.json()) as User;
    }

    async listPosts(authorId: string): Promise<Post[]> {
        const res = await fetch(`${this.baseUrl}/posts?author=${authorId}`, {
            headers: this.headers(),
        });
        return (await res.json()) as Post[];
    }

    async createPost(p: Omit<Post, "id">): Promise<Post> {
        const res = await fetch(`${this.baseUrl}/posts`, {
            method: "POST",
            headers: this.headers(),
            body: JSON.stringify(p),
        });
        return (await res.json()) as Post;
    }
}

export function useUser(client: ApiClient, id: string): Loadable<User> {
    const [state, setState] = useState<Loadable<User>>({ status: "idle" });

    useEffect(() => {
        let cancelled = false;
        setState({ status: "loading" });
        client
            .getUser(id)
            .then((data) => {
                if (!cancelled) setState({ status: "success", data });
            })
            .catch((err: Error) => {
                if (!cancelled) setState({ status: "error", message: err.message });
            });
        return () => {
            cancelled = true;
        };
    }, [client, id]);

    return state;
}

export function usePostsByTag(posts: Post[], tag: string): Post[] {
    return useMemo(() => posts.filter((p) => p.tags.includes(tag)), [posts, tag]);
}

export function useCreatePost(
    client: ApiClient,
    setPosts: Dispatch<SetStateAction<Post[]>>,
): (p: Omit<Post, "id">) => Promise<void> {
    return useCallback(
        async (p) => {
            const created = await client.createPost(p);
            setPosts((prev) => [...prev, created]);
        },
        [client, setPosts],
    );
}

export const DEFAULT_CLIENT = new ApiClient("/api/v1");
