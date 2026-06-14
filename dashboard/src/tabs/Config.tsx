import { useEffect, useState } from "react";
import { Button } from "@base-ui/react/button";
import { Field } from "@base-ui/react/field";
import { useForm } from "react-hook-form";
import useSWR from "swr";
import { errMessage, mutateRequest, textFetcher } from "../lib/http";

interface ConfigForm {
  config: string;
}

export function Config() {
  const { data, error, isLoading, mutate } = useSWR<string>(
    "/api/config",
    textFetcher,
  );
  const {
    register,
    handleSubmit,
    reset,
    setError,
    formState: { isDirty, isSubmitting, errors },
  } = useForm<ConfigForm>({ defaultValues: { config: "" } });
  const [saved, setSaved] = useState(false);

  // Adopt the loaded TOML as the form baseline so `isDirty` tracks real edits.
  useEffect(() => {
    if (data !== undefined) reset({ config: data });
  }, [data, reset]);

  const onSubmit = handleSubmit(async ({ config }) => {
    setSaved(false);
    try {
      await mutateRequest("/api/config", {
        method: "PUT",
        body: config,
        contentType: "application/toml",
      });
      await mutate(); // re-GET the canonical stored TOML → resets the baseline
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError("root", { message: errMessage(e) });
    }
  });

  return (
    <section>
      <header className="events-header">
        <h2>Configuration</h2>
        <div className="filters">
          {errors.root?.message && (
            <span className="error">{errors.root.message}</span>
          )}
          {saved && <span className="conn ok">saved</span>}
          <Button
            type="button"
            onClick={onSubmit}
            disabled={!isDirty || isSubmitting || isLoading}
          >
            {isSubmitting ? "Saving…" : "Save"}
          </Button>
        </div>
      </header>
      {error && <p className="error">Failed to load: {errMessage(error)}</p>}
      <Field.Root>
        <Field.Control
          className="config-editor"
          disabled={isLoading || data === undefined}
          render={<textarea spellCheck={false} />}
          {...register("config")}
        />
      </Field.Root>
    </section>
  );
}
