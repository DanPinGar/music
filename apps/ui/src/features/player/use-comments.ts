import { type Comment, type CommentFields, expectData } from "@segno/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApi } from "@/api/api-context";
import { type CommentItem, toCommentItems } from "./comments";

export interface CommentActions {
  readonly items: readonly CommentItem[];
  add(fields: CommentFields): void;
  change(id: string, fields: CommentFields): void;
  remove(id: string): void;
}

/** The video's comments, shown changed at once and rolled back if the core refuses. */
export function useComments(videoId: string): CommentActions {
  const { t } = useTranslation();
  const api = useApi();
  const queryClient = useQueryClient();
  const key = ["comments", videoId];
  const path = { params: { path: { id: videoId } } };

  const query = useQuery({
    queryKey: key,
    queryFn: async () => expectData(await api.GET("/videos/{id}/comments", path)),
  });

  const optimistic = async (update: (comments: Comment[]) => Comment[]) => {
    await queryClient.cancelQueries({ queryKey: key });
    const before = queryClient.getQueryData<Comment[]>(key) ?? [];
    queryClient.setQueryData<Comment[]>(key, update(before));
    return { before };
  };
  const settle = {
    onError: (error: Error, _: unknown, context?: { before: Comment[] }) => {
      if (context) queryClient.setQueryData(key, context.before);
      toast.error(t("comments.saveFailed"), { description: error.message });
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: key }),
  };
  const pending = (fields: CommentFields, id: string): Comment => ({
    id,
    start_ms: fields.start_ms,
    end_ms: fields.end_ms ?? null,
    body: fields.body.trim(),
    created_at: "",
    updated_at: "",
  });

  const add = useMutation({
    mutationFn: async (fields: CommentFields) =>
      expectData(await api.POST("/videos/{id}/comments", { ...path, body: fields })),
    onMutate: (fields: CommentFields) =>
      optimistic((comments) => [...comments, pending(fields, `pending-${Date.now()}`)]),
    ...settle,
  });

  const change = useMutation({
    mutationFn: async ({ id, fields }: { id: string; fields: CommentFields }) =>
      expectData(
        await api.PUT("/videos/{id}/comments/{comment_id}", {
          params: { path: { id: videoId, comment_id: id } },
          body: fields,
        }),
      ),
    onMutate: ({ id, fields }) =>
      optimistic((comments) =>
        comments.map((comment) => (comment.id === id ? pending(fields, id) : comment)),
      ),
    ...settle,
  });

  const remove = useMutation({
    mutationFn: async (id: string) => {
      const result = await api.DELETE("/videos/{id}/comments/{comment_id}", {
        params: { path: { id: videoId, comment_id: id } },
      });
      if (result.error !== undefined) expectData(result);
    },
    onMutate: (id: string) =>
      optimistic((comments) => comments.filter((comment) => comment.id !== id)),
    ...settle,
  });

  const items = useMemo(() => toCommentItems(query.data ?? []), [query.data]);
  return {
    items,
    add: (fields) => add.mutate(fields),
    change: (id, fields) => change.mutate({ id, fields }),
    remove: (id) => remove.mutate(id),
  };
}
